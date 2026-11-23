package com.filedrop.app

import android.content.ActivityNotFoundException
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.PowerManager
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.Settings
import androidx.core.content.FileProvider
import java.io.File

/**
 * Turns a folder the user chose into a path the transfer server may write to.
 *
 * Since Android 10 an app cannot simply create a file in shared storage: the
 * path belongs to MediaStore, which hands out the file (and its real path) when
 * a row is inserted. The row is left empty on purpose — the Rust side streams
 * the bytes into the path this returns, and an app is always allowed to write
 * its own files.
 *
 * Anything MediaStore cannot answer for — a folder on another volume, a release
 * before Android 10 — falls back to the plain filesystem, which is the only
 * thing those devices have.
 */
object StorageBridge {

  /** The provider that serves the folders under primary shared storage. */
  private const val EXTERNAL_STORAGE_AUTHORITY = "com.android.externalstorage.documents"

  /** What went wrong in the last claim; empty after a successful one. */
  private var lastFailure: String = ""

  /** Primary shared storage, the root the settings page builds paths under. */
  @JvmStatic
  fun externalRoot(): String = Environment.getExternalStorageDirectory().absolutePath

  /**
   * Where the activity parks files another app shared, for the Rust side to
   * drain. Defined here so both sides ask the same question instead of each
   * guessing which directory `dataDir` means. External app storage keeps the
   * copies off the internal data partition (a shared video can be large) while
   * staying private to the app.
   */
  @JvmStatic
  fun shareInbox(context: Context): String {
    val base = context.getExternalFilesDir(null) ?: context.dataDir
    return File(base, "share-inbox").absolutePath
  }

  /** Why the last claim failed, for the receive side to tell the user. */
  @JvmStatic
  fun lastError(): String = lastFailure

  /**
   * Whether the system lets this app use the network while the screen is off.
   *
   * Doze suspends networking for every app that is not exempted, wake locks and
   * foreground services included; the phone then answers pings and refuses
   * every connection, which is exactly what "online but cannot send" looks like
   * from the other side.
   */
  @JvmStatic
  fun ignoringBatteryOptimizations(context: Context): Boolean {
    val power = context.getSystemService(Context.POWER_SERVICE) as PowerManager
    return power.isIgnoringBatteryOptimizations(context.packageName)
  }

  /** Ask for that exemption; false means no screen on this device would open. */
  @JvmStatic
  fun requestBatteryExemption(context: Context): Boolean {
    return try {
      context.startActivity(
        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
          .setData(Uri.parse("package:${context.packageName}"))
          .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      )
      true
    } catch (error: Throwable) {
      try {
        context.startActivity(
          Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
        true
      } catch (fallback: Throwable) {
        false
      }
    }
  }

  /**
   * Open `path` with whichever app the user has for it.
   *
   * Null means the file is on its way to another app; anything else explains
   * why not. Doing this here rather than through JNI keeps the provider lookup
   * on the app's own class loader and lets every failure name itself — the
   * generic "could not open" left no way to tell a missing provider root from
   * a missing viewer.
   */
  @JvmStatic
  fun openFile(context: Context, path: String): String? {
    val file = File(path)
    if (!file.isFile) return "这个文件已经不在这台手机上了"
    val uri = try {
      FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
    } catch (error: Throwable) {
      return "无法把这个文件交给其他应用（${error.javaClass.simpleName}: ${error.message}）"
    }
    val mime = mimeOf(path)
    val intent = Intent(Intent.ACTION_VIEW)
      .setDataAndType(uri, mime)
      .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      null
    } catch (error: ActivityNotFoundException) {
      "没有能打开这种文件的应用（$mime）"
    } catch (error: Throwable) {
      "打开失败：${error.javaClass.simpleName}: ${error.message}"
    }
  }

  private fun mimeOf(path: String): String =
    when (path.substringAfterLast('.', "").lowercase()) {
      "png" -> "image/png"
      "jpg", "jpeg" -> "image/jpeg"
      "gif" -> "image/gif"
      "webp" -> "image/webp"
      "bmp" -> "image/bmp"
      "mp4" -> "video/mp4"
      "mov" -> "video/quicktime"
      "mkv" -> "video/x-matroska"
      "mp3" -> "audio/mpeg"
      "m4a" -> "audio/mp4"
      "wav" -> "audio/x-wav"
      "pdf" -> "application/pdf"
      "txt", "md", "log" -> "text/plain"
      "zip" -> "application/zip"
      "apk" -> "application/vnd.android.package-archive"
      else -> "*/*"
    }

  /**
   * The absolute path to write `name` into `dir` to, or null when nothing could
   * be claimed. A collision is not an error: MediaStore renames the new file and
   * the renamed path is what comes back.
   *
   * The file is created here, empty: MediaStore only puts the bytes on disk when
   * something opens the path for writing, and a row whose file never appears is
   * exactly the phantom entry the gallery would keep showing.
   */
  @JvmStatic
  fun claim(context: Context, dir: String, name: String): String? {
    lastFailure = ""
    if (name.isBlank()) return null
    val root = Environment.getExternalStorageDirectory().absolutePath
    val underSharedStorage = dir == root || dir.startsWith("$root${File.separator}")
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && underSharedStorage) {
      try {
        claimThroughMediaStore(context, root, dir, name)?.let { return it }
      } catch (error: Throwable) {
        lastFailure = "MediaStore: ${error.javaClass.simpleName}: ${error.message}"
      }
    }
    return try {
      claimOnDisk(dir, name)
    } catch (error: Throwable) {
      if (lastFailure.isEmpty()) {
        lastFailure = "${error.javaClass.simpleName}: ${error.message}"
      }
      null
    }
  }

  private fun claimThroughMediaStore(
    context: Context,
    root: String,
    dir: String,
    name: String
  ): String? {
    val relative = dir.removePrefix(root).trim('/')
    val values = ContentValues().apply {
      put(MediaStore.Downloads.DISPLAY_NAME, name)
      put(MediaStore.Downloads.MIME_TYPE, "application/octet-stream")
      // RELATIVE_PATH is a directory inside a standard folder; a leading slash
      // makes the row fail to insert on some releases.
      if (relative.isNotEmpty()) put(MediaStore.Downloads.RELATIVE_PATH, relative)
    }
    val uri = context.contentResolver
      // Inserts are refused on the merged volume; only the primary one can take
      // a new row. `VOLUME_EXTERNAL` made every claim fall through to the plain
      // path write, so the file landed but the gallery never learned about it.
      .insert(MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY), values)
    if (uri == null) {
      lastFailure = "系统拒绝在下载目录登记这个文件"
      return null
    }
    val columns = arrayOf(MediaStore.Downloads.DATA)
    val path = context.contentResolver.query(uri, columns, null, null, null)?.use { cursor ->
      if (!cursor.moveToFirst()) return@use null
      val index = cursor.getColumnIndex(MediaStore.Downloads.DATA)
      if (index < 0 || cursor.isNull(index)) null else cursor.getString(index)
    }
    if (path == null) {
      // A row without a path is a lie the gallery would keep showing: drop it and
      // let the caller fall back to somewhere the file really lands.
      context.contentResolver.delete(uri, null, null)
      lastFailure = "系统没有返回这个文件的可写路径"
      return null
    }
    val writable = try {
      File(path).outputStream().use { }
      true
    } catch (error: Throwable) {
      lastFailure = "路径不可写: ${error.javaClass.simpleName}: ${error.message}"
      false
    }
    if (!writable) {
      context.contentResolver.delete(uri, null, null)
      return null
    }
    return path
  }

  /**
   * Show `dir` in the system's file browser.
   *
   * A folder in shared storage is reached through the external-storage document
   * provider rather than a `file://` path, which the platform refuses to hand to
   * another app. False means no app on this device will show it, and the caller
   * says where the files are instead.
   */
  @JvmStatic
  fun openFolder(context: Context, dir: String): Boolean {
    val root = Environment.getExternalStorageDirectory().absolutePath
    if (dir != root && !dir.startsWith("$root${File.separator}")) return false
    val relative = dir.removePrefix(root).trim('/')
    val documentId = if (relative.isEmpty()) "primary" else "primary:$relative"
    val uri = DocumentsContract.buildDocumentUri(EXTERNAL_STORAGE_AUTHORITY, documentId)
    val intent = Intent(Intent.ACTION_VIEW, uri)
      .setType(DocumentsContract.Document.MIME_TYPE_DIR)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      true
    } catch (error: Exception) {
      false
    }
  }

  /**
   * The plain-filesystem answer: make sure the folder exists and hand back a
   * name that does not collide with what is already there. Used on release 9 and
   * below, where the app may create shared files itself, and as the last resort
   * when MediaStore has no row to offer.
   */
  private fun claimOnDisk(dir: String, name: String): String? {
    val folder = File(dir)
    if (!folder.isDirectory && !folder.mkdirs()) return null
    val candidate = File(folder, name)
    if (!candidate.exists()) return candidate.absolutePath
    val stem = name.substringBeforeLast('.', name)
    val extension = name.substringAfterLast('.', "")
    for (index in 1..999) {
      val next = if (extension.isEmpty()) {
        File(folder, "$stem ($index)")
      } else {
        File(folder, "$stem ($index).$extension")
      }
      if (!next.exists()) return next.absolutePath
    }
    return null
  }
}
