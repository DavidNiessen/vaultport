// SPDX-License-Identifier: GPL-3.0-or-later
package dev.niessen.sshclient.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/**
 * Foreground service that keeps the process (and therefore open SSH sessions and port
 * forwards) alive while the app is in the background. It does no work itself; it only
 * shows a persistent notification so Android does not kill the app.
 */
class KeepAliveService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val sessions = intent?.getIntExtra(EXTRA_SESSIONS, 1) ?: 1
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL, "Active connections", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Keeps SSH sessions and tunnels open in the background"
                    setShowBadge(false)
                },
            )
        }
        val launch = packageManager.getLaunchIntentForPackage(packageName)
        val pending = PendingIntent.getActivity(
            this, 0, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val text = if (sessions == 1) "1 connection is open" else "$sessions connections are open"
        val notification: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setContentTitle("ssh-client")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.stat_sys_upload_done)
            .setOngoing(true)
            .setContentIntent(pending)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            notification,
            if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0,
        )
        return START_NOT_STICKY
    }

    companion object {
        const val EXTRA_SESSIONS = "sessions"
        private const val CHANNEL = "ssh-client-connections"
        private const val NOTIFICATION_ID = 4022
    }
}
