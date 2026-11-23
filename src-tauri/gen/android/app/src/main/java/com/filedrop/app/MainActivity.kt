package com.filedrop.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.io.File

class MainActivity : TauriActivity() {
  private var keyboardInset = 0
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    requestNotificationPermission()
    requestStoragePermission()
    keepComposerAboveKeyboard()
    collectSharedFiles(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    collectSharedFiles(intent)
  }

  // The page owns navigation, not the activity. wry installs a back callback that
  // finishes the activity as soon as the webview cannot go back — which dropped
  // the user onto the launcher from a chat or from the 我的 page. This callback is
  // registered later, so it runs first and lets the page decide; only an
  // unhandled back falls through to wry's finish.
  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    onBackPressedDispatcher.addCallback(
      this,
      object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() {
          webView.evaluateJavascript(
            "(window.__filedropBack ? window.__filedropBack() : false)"
          ) { answer ->
            if (answer != "true") {
              isEnabled = false
              this@MainActivity.onBackPressed()
              isEnabled = true
            }
          }
        }
      }
    )
  }

  // The system share sheet hands over content:// URIs, which the receive side
  // cannot read by path and which are only valid while this activity lives. They
  // are copied into the share inbox first; the page takes them from there.
  private fun collectSharedFiles(intent: Intent?) {
    val action = intent?.action ?: return
    if (action != Intent.ACTION_SEND && action != Intent.ACTION_SEND_MULTIPLE) return
    @Suppress("DEPRECATION")
    val uris: List<Uri> =
      if (action == Intent.ACTION_SEND) {
        listOfNotNull(intent.getParcelableExtra(Intent.EXTRA_STREAM))
      } else {
        intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM).orEmpty()
      }
    if (uris.isEmpty()) return
    // The same intent comes back after a configuration change; clearing the
    // extra keeps the batch from being copied (and offered) twice.
    intent.removeExtra(Intent.EXTRA_STREAM)

    Thread {
      val inbox = File(StorageBridge.shareInbox(this)).apply { mkdirs() }
      val saved = uris.count { copyShared(it, inbox) }
      if (saved == 0) return@Thread
      runOnUiThread {
        webView?.evaluateJavascript(
          "(window.__filedropShared ? window.__filedropShared() : false)",
          null
        )
      }
    }.start()
  }

  private fun copyShared(uri: Uri, inbox: File): Boolean {
    val name = displayName(uri)?.takeIf { it.isNotBlank() }
      ?: "shared-${System.currentTimeMillis()}"
    val clean = name.replace(Regex("[/\\\\:*?\"<>|]"), "_")
    val target = freeName(inbox, clean)
    return try {
      contentResolver.openInputStream(uri)?.use { input ->
        target.outputStream().use { output -> input.copyTo(output) }
        true
      } ?: false
    } catch (error: Exception) {
      false
    }
  }

  private fun freeName(inbox: File, name: String): File {
    var candidate = File(inbox, name)
    if (!candidate.exists()) return candidate
    val stem = name.substringBeforeLast('.', name)
    val extension = name.substringAfterLast('.', "")
    var index = 1
    while (candidate.exists()) {
      candidate =
        if (extension.isEmpty()) File(inbox, "$stem ($index)")
        else File(inbox, "$stem ($index).$extension")
      index++
    }
    return candidate
  }

  private fun displayName(uri: Uri): String? {
    return try {
      contentResolver
        .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
        ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
    } catch (error: Exception) {
      null
    }
  }

  // An edge-to-edge window is not resized by the on-screen keyboard any more
  // (windowSoftInputMode only mattered while the window still fitted the system
  // bars), so the page kept its full height and the composer stayed under the
  // IME. Padding the root by the keyboard inset shrinks the view the webview
  // lives in, which is what the old adjustResize did to the whole window, and
  // consuming the bottom inset stops the page adding it a second time.
  private fun keepComposerAboveKeyboard() {
    ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { view, insets ->
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom
      if (ime != keyboardInset) {
        keyboardInset = ime
        view.setPadding(view.paddingLeft, view.paddingTop, view.paddingRight, ime)
      }
      insets.inset(0, 0, 0, ime)
    }
  }

  // Android 13+ needs a runtime grant before the inbound file/message
  // notifications the Rust notification plugin posts can actually appear.
  private fun requestNotificationPermission() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    val granted =
      ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) ==
        PackageManager.PERMISSION_GRANTED
    if (!granted) {
      ActivityCompat.requestPermissions(
        this,
        arrayOf(Manifest.permission.POST_NOTIFICATIONS),
        NOTIFICATION_REQUEST
      )
    }
  }

  // Below Android 10 the platform will not hand out a shared-storage path for a
  // new file, so the receive path writes by path — which the storage permission
  // has to allow. From 10 on the permission does not exist any more and
  // StorageBridge claims the file through MediaStore instead.
  private fun requestStoragePermission() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) return
    val granted =
      ContextCompat.checkSelfPermission(this, Manifest.permission.WRITE_EXTERNAL_STORAGE) ==
        PackageManager.PERMISSION_GRANTED
    if (!granted) {
      ActivityCompat.requestPermissions(
        this,
        arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE),
        STORAGE_REQUEST
      )
    }
  }

  companion object {
    private const val NOTIFICATION_REQUEST = 1001
    private const val STORAGE_REQUEST = 1002
  }
}
