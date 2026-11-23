//! Android-only native plumbing.
//!
//! Everything that has to hold state for the life of the process — the Wi-Fi
//! multicast lock, the wake lock, the persistent notification, the runtime
//! notification permission — lives in Kotlin (`ServerService` / `MainActivity`)
//! where the platform guarantees it. This module only covers the operations the
//! webview asks for on demand: open a link, open a received file, and start or
//! stop the foreground service that keeps the receive server alive.
//!
//! Those need the Android `Activity`, reachable through the webview's JNI
//! handle. `exec` dispatches the closure to the UI thread and returns nothing,
//! so a channel carries the real result back to the (worker-thread) caller.

use jni::objects::{GlobalRef, JObject, JString, JValue};
use jni::JNIEnv;
use tauri::{AppHandle, Manager};

/// Fully-qualified Kotlin service class declared in AndroidManifest.xml.
const SERVER_SERVICE: &str = "com.filedrop.app.ServerService";

/// Kotlin helper that owns the shared-storage answers (see `StorageBridge.kt`).
const STORAGE_BRIDGE: &str = "com.filedrop.app.StorageBridge";

/// Turn a pending Java exception into a Rust error and clear it, so a failed
/// JNI call never leaves the VM in a state that poisons the next call.
fn pending(env: &mut JNIEnv, message: &str) -> Result<(), String> {
    if env.exception_check().map_err(|error| error.to_string())? {
        let _ = env.exception_clear();
        return Err(message.to_string());
    }
    Ok(())
}

fn jstring<'e>(env: &mut JNIEnv<'e>, value: &str) -> Result<JObject<'e>, String> {
    env.new_string(value)
        .map_err(|error| error.to_string())
        .map(|string| string.into())
}

/// Resolve a class through the app's own loader.
///
/// `FindClass` (what a plain class-name argument to `call_static_method` ends up
/// doing) only sees the boot class path on the thread this code runs on — wry
/// executes the closure on its own JNI-attached thread, which has no Java frame
/// to borrow a loader from. Any class of this package, or of an AndroidX
/// library, then comes back as `NoClassDefFoundError` instead of being found.
/// The activity's loader sees all of them.
fn app_class(env: &mut JNIEnv, context: &JObject, name: &str) -> Result<GlobalRef, String> {
    let loader = env
        .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .map_err(|error| error.to_string())?
        .l()
        .map_err(|error| error.to_string())?;
    let name = env
        .new_string(name)
        .map_err(|error| error.to_string())?;
    let class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )
        .map_err(|error| error.to_string())?
        .l()
        .map_err(|error| error.to_string())?;
    env.new_global_ref(class).map_err(|error| error.to_string())
}

/// Open an http(s) link in the browser the user chose.
pub fn open_url(app: &AppHandle, url: String) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<(), String> {
                    let action = jstring(env, "android.intent.action.VIEW")?;
                    let intent = env
                        .new_object(
                            "android/content/Intent",
                            "(Ljava/lang/String;)V",
                            &[JValue::Object(&action)],
                        )
                        .map_err(|error| error.to_string())?;
                    let target = jstring(env, &url)?;
                    let uri = env
                        .call_static_method(
                            "android/net/Uri",
                            "parse",
                            "(Ljava/lang/String;)Landroid/net/Uri;",
                            &[JValue::Object(&target)],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法解析链接")?;
                    env.call_method(
                        &intent,
                        "setData",
                        "(Landroid/net/Uri;)Landroid/content/Intent;",
                        &[JValue::Object(&uri)],
                    )
                    .map_err(|error| error.to_string())?;
                    pending(env, "无法打开链接")?;
                    env.call_method(
                        activity,
                        "startActivity",
                        "(Landroid/content/Intent;)V",
                        &[JValue::Object(&intent)],
                    )
                    .map_err(|error| error.to_string())?;
                    pending(env, "没有可以打开链接的应用")?;
                    Ok(())
                })();
                // A pending Java exception left behind by a failed call would
                // otherwise escape this closure and take the whole app down
                // inside the webview's JNI bridge, so it is always cleared here
                // and turned into the error the caller gets.
                let _ = tx.send(pending(env, "无法打开链接").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Open a file the page asked for with whatever app handles it.
///
/// The work lives in Kotlin (`StorageBridge.openFile`): the provider lookup and
/// the VIEW intent both belong on the app's class loader, and every failure
/// there names itself, which a JNI-side error could not.
pub fn open_file(app: &AppHandle, path: String) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<(), String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let bridge = app_class(env, &context, STORAGE_BRIDGE)?;
                    let path_arg = jstring(env, &path)?;
                    let reason = env
                        .call_static_method(
                            &bridge,
                            "openFile",
                            "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                            &[JValue::Object(&context), JValue::Object(&path_arg)],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法打开文件")?;
                    if reason.is_null() {
                        return Ok(());
                    }
                    let message: String = env
                        .get_string(&JString::from(reason))
                        .map_err(|error| error.to_string())?
                        .into();
                    Err(message)
                })();
                let _ = tx.send(pending(env, "无法打开文件").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Why the last claim returned nothing, as the platform phrased it.
pub fn claim_error(app: &AppHandle) -> Result<String, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<String, String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let bridge = app_class(env, &context, STORAGE_BRIDGE)?;
                    let reason = env
                        .call_static_method(&bridge, "lastError", "()Ljava/lang/String;", &[])
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法读取保存失败的原因")?;
                    Ok(env
                        .get_string(&JString::from(reason))
                        .map_err(|error| error.to_string())?
                        .into())
                })();
                let _ = tx.send(pending(env, "无法读取保存失败的原因").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Where the platform allows `name` to be written inside `dir`.
///
/// Android 10+ will not let this process create a file in shared storage by
/// itself — the row belongs to MediaStore — so the receive path asks first and
/// writes to whatever path comes back. `None` means the platform claimed
/// nothing and the caller has somewhere else to put the file.
pub fn claim_landing_path(app: &AppHandle, dir: &str, name: &str) -> Result<Option<String>, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<Option<String>, String>>();
    let dir = dir.to_string();
    let name = name.to_string();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<Option<String>, String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let dir_arg = jstring(env, &dir)?;
                    let name_arg = jstring(env, &name)?;
                    let bridge = app_class(env, &context, STORAGE_BRIDGE)?;
                    let claimed = env
                        .call_static_method(
                            &bridge,
                            "claim",
                            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                            &[
                                JValue::Object(&context),
                                JValue::Object(&dir_arg),
                                JValue::Object(&name_arg),
                            ],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法确认保存位置")?;
                    if claimed.is_null() {
                        return Ok(None);
                    }
                    Ok(Some(
                        env.get_string(&JString::from(claimed))
                            .map_err(|error| error.to_string())?
                            .into(),
                    ))
                })();
                // A pending Java exception left behind by a failed call would
                // otherwise escape this closure and take the whole app down
                // inside the webview's JNI bridge, so it is always cleared here
                // and turned into the error the caller gets.
                let _ = tx.send(pending(env, "无法确认保存位置").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Primary shared storage, the root the settings page offers folders under.
pub fn external_root(app: &AppHandle) -> Result<String, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<String, String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let bridge = app_class(env, &context, STORAGE_BRIDGE)?;
                    let root = env
                        .call_static_method(&bridge, "externalRoot", "()Ljava/lang/String;", &[])
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法读取存储位置")?;
                    Ok(env
                        .get_string(&JString::from(root))
                        .map_err(|error| error.to_string())?
                        .into())
                })();
                // A pending Java exception left behind by a failed call would
                // otherwise escape this closure and take the whole app down
                // inside the webview's JNI bridge, so it is always cleared here
                // and turned into the error the caller gets.
                let _ = tx.send(pending(env, "无法读取存储位置").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Where the activity parks files another app shared, for the send sheet.
pub fn share_inbox(app: &AppHandle) -> Result<String, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<String, String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let bridge = app_class(env, &context, STORAGE_BRIDGE)?;
                    let inbox = env
                        .call_static_method(
                            &bridge,
                            "shareInbox",
                            "(Landroid/content/Context;)Ljava/lang/String;",
                            &[JValue::Object(&context)],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法读取分享目录")?;
                    Ok(env
                        .get_string(&JString::from(inbox))
                        .map_err(|error| error.to_string())?
                        .into())
                })();
                let _ = tx.send(pending(env, "无法读取分享目录").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Show `dir` in the system file browser.
///
/// False means no app on this device will display that folder and the caller
/// says where the files are instead.
pub fn open_folder(app: &AppHandle, dir: &str) -> Result<bool, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<bool, String>>();
    let dir = dir.to_string();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<bool, String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let dir_arg = jstring(env, &dir)?;
                    let bridge = app_class(env, &context, STORAGE_BRIDGE)?;
                    let shown = env
                        .call_static_method(
                            &bridge,
                            "openFolder",
                            "(Landroid/content/Context;Ljava/lang/String;)Z",
                            &[JValue::Object(&context), JValue::Object(&dir_arg)],
                        )
                        .map_err(|error| error.to_string())?
                        .z()
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法打开保存位置")?;
                    Ok(shown)
                })();
                // A pending Java exception left behind by a failed call would
                // otherwise escape this closure and take the whole app down
                // inside the webview's JNI bridge, so it is always cleared here
                // and turned into the error the caller gets.
                let _ = tx.send(pending(env, "无法打开保存位置").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Whether the system exempts this app from Doze's network suspension.
pub fn battery_exempt(app: &AppHandle) -> Result<bool, String> {
    battery_call(app, "ignoringBatteryOptimizations", "无法读取后台运行状态")
}

/// Ask for the exemption; false means no settings screen accepted the intent.
pub fn request_battery_exemption(app: &AppHandle) -> Result<bool, String> {
    battery_call(app, "requestBatteryExemption", "无法打开电池优化设置")
}

/// Both battery calls share one shape: a static Kotlin helper that takes the
/// application context and answers with a boolean.
fn battery_call(app: &AppHandle, method: &str, failure: &'static str) -> Result<bool, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<bool, String>>();
    let method = method.to_string();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<bool, String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let bridge = app_class(env, &context, STORAGE_BRIDGE)?;
                    let answer = env
                        .call_static_method(
                            &bridge,
                            &method,
                            "(Landroid/content/Context;)Z",
                            &[JValue::Object(&context)],
                        )
                        .map_err(|error| error.to_string())?
                        .z()
                        .map_err(|error| error.to_string())?;
                    pending(env, failure)?;
                    Ok(answer)
                })();
                let _ = tx.send(pending(env, failure).and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}

/// Keep the receive server alive while the app is in the background.
pub fn start_background_service(app: &AppHandle) -> Result<(), String> {
    send_service_intent(app, "startForegroundService")
}

/// Release the persistent notification once receiving stops.
pub fn stop_background_service(app: &AppHandle) -> Result<(), String> {
    send_service_intent(app, "stopService")
}

fn send_service_intent(app: &AppHandle, method: &str) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "没有可用的窗口".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let method = method.to_string();
    window
        .with_webview(move |webview| {
            let method = method.clone();
            webview.jni_handle().exec(move |env, activity, _webview| {
                let result = (|| -> Result<(), String> {
                    let context = env
                        .call_method(
                            activity,
                            "getApplicationContext",
                            "()Landroid/content/Context;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let intent = env
                        .new_object("android/content/Intent", "()V", &[])
                        .map_err(|error| error.to_string())?;
                    let service = jstring(env, SERVER_SERVICE)?;
                    env.call_method(
                        &intent,
                        "setClassName",
                        "(Landroid/content/Context;Ljava/lang/String;)Landroid/content/Intent;",
                        &[JValue::Object(&context), JValue::Object(&service)],
                    )
                    .map_err(|error| error.to_string())?;
                    let (name, signature) = if method == "stopService" {
                        ("stopService", "(Landroid/content/Intent;)Z")
                    } else {
                        (
                            "startForegroundService",
                            "(Landroid/content/Intent;)Landroid/content/ComponentName;",
                        )
                    };
                    env.call_method(&context, name, signature, &[JValue::Object(&intent)])
                        .map_err(|error| error.to_string())?;
                    pending(env, "无法启动后台接收服务")?;
                    Ok(())
                })();
                // A pending Java exception left behind by a failed call would
                // otherwise escape this closure and take the whole app down
                // inside the webview's JNI bridge, so it is always cleared here
                // and turned into the error the caller gets.
                let _ = tx.send(pending(env, "无法启动后台接收服务").and(result));
            });
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "等待界面线程超时".to_string())?
}


