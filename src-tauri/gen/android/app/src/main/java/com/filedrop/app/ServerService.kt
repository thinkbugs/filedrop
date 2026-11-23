package com.filedrop.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager

/**
 * Keeps the FileDrop process resident while the transfer server is running, so
 * LAN receiving works with the screen off — the desktop equivalent of "always
 * ready for files". The Rust side starts and stops this together with the
 * server; the actual sockets live in the Rust runtime threads this service only
 * keeps alive.
 *
 * While running it also holds the two platform locks the desktop build never
 * had to ask for: the Wi-Fi multicast lock (mDNS peer discovery is otherwise
 * silently dropped) and a partial wake lock (the CPU stays awake to answer).
 */
class ServerService : Service() {

  private var multicastLock: WifiManager.MulticastLock? = null
  private var wifiLock: WifiManager.WifiLock? = null
  private var wakeLock: PowerManager.WakeLock? = null

  override fun onCreate() {
    super.onCreate()
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    manager.createNotificationChannel(
      NotificationChannel(CHANNEL_ID, "FileDrop 接收", NotificationManager.IMPORTANCE_LOW)
    )

    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
    multicastLock = wifi.createMulticastLock("filedrop").apply {
      setReferenceCounted(false)
      acquire()
    }
    // The CPU wake lock keeps the process awake, but the Wi-Fi radio still drops
    // into power save on its own — the other side then finds nothing listening
    // and calls this phone offline, even though discovery still lists it.
    @Suppress("DEPRECATION")
    wifiLock = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "filedrop:wifi").apply {
      setReferenceCounted(false)
      acquire()
    }
    wakeLock =
      (applicationContext.getSystemService(Context.POWER_SERVICE) as PowerManager)
        .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "filedrop:server")
        .apply { acquire() }
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    // A null intent means the system restarted us after the process was killed.
    // The server lived in that process, so nothing is receiving any more;
    // claiming otherwise would keep a notification, the CPU wake lock and the
    // multicast lock alive for a device that cannot answer a single packet.
    if (intent == null) {
      stopSelf()
      return START_NOT_STICKY
    }
    val notification = buildNotification()
    // The typed overload only exists from API 29; before that the service simply
    // declares its type in the manifest and nothing checks it.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      startForeground(NOTIFICATION_ID, notification, foregroundType())
    } else {
      startForeground(NOTIFICATION_ID, notification)
    }
    return START_STICKY
  }

  // Android 14 stops a dataSync service after 6 hours in any 24, and the phone
  // silently leaves the network the moment it does — for a receiver that is the
  // whole product, so it asks for the uncapped specialUse from 34 up. The type
  // has to be one the platform on that device knows about, hence the split;
  // below 34 dataSync is only informational and carries no such quota.
  private fun foregroundType(): Int {
    return if (Build.VERSION.SDK_INT >= 34) {
      ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
    } else {
      ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
    }
  }

  private fun buildNotification(): Notification {
    val builder =
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        Notification.Builder(this, CHANNEL_ID)
      } else {
        @Suppress("DEPRECATION") Notification.Builder(this)
      }
    return builder
      .setContentTitle("FileDrop 正在接收")
      .setContentText("其他设备可以向本机发送文件")
      .setSmallIcon(android.R.drawable.stat_sys_download)
      .setOngoing(true)
      .build()
  }

  override fun onDestroy() {
    multicastLock?.takeIf { it.isHeld }?.release()
    wifiLock?.takeIf { it.isHeld }?.release()
    wakeLock?.takeIf { it.isHeld }?.release()
    super.onDestroy()
  }

  override fun onBind(intent: Intent?): IBinder? = null

  companion object {
    private const val CHANNEL_ID = "filedrop_server"
    private const val NOTIFICATION_ID = 1001
  }
}
