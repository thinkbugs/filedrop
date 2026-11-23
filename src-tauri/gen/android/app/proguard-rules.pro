# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# StorageBridge is reached only from Rust through JNI: no class in the dex names
# it, so the manifest-based rules that protect the activity and the service do
# not apply, and shrinking alone decides whether the feature exists. The arm64
# APK built at 03:22 is the proof — the class was left as an empty shell (no
# static fields, no methods), so every claim would have failed and silently
# fallen back to the private directory.
-keep class com.filedrop.app.StorageBridge {
  *;
}