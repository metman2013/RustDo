package io.github.metman2013.rustdo;

import android.app.AlarmManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.os.Build;

import org.json.JSONArray;
import org.json.JSONObject;

import java.text.ParseException;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;

/**
 * Reminders while RustDo is closed: each upcoming reminder in tasks.json gets an alarm;
 * when it goes off and RustDo isn't on screen, a notification shows it. (On screen, RustDo
 * shows its own reminder pop-up.) Scheduled again whenever the tasks change, RustDo is
 * left, the phone restarts or RustDo is updated.
 */
final class Reminders {
    private Reminders() {}

    static final String CHANNEL = "reminders";
    /** Only the nearest ones get an alarm (they're scheduled again as they pass). */
    private static final int MAX_ALARMS = 50;

    /** A reminder waiting: task id, when (ms), the reminder text as saved (to check it's unchanged). */
    private static final class Upcoming {
        final String id;
        final long at;
        final String when;

        Upcoming(String id, long at, String when) {
            this.id = id;
            this.at = at;
            this.when = when;
        }
    }

    static long parse(String naive) {
        if (naive == null || naive.length() < 16) return -1;
        try {
            return new SimpleDateFormat("yyyy-MM-dd'T'HH:mm", Locale.ROOT).parse(naive.substring(0, 16)).getTime();
        } catch (ParseException e) {
            return -1;
        }
    }

    private static List<Upcoming> upcoming(Context c) {
        List<Upcoming> out = new ArrayList<>();
        JSONObject store = TaskStore.load(c);
        if (store == null) return out;
        JSONObject settings = store.optJSONObject("settings");
        if (settings != null && !settings.optBoolean("notifications", true)) return out;
        JSONArray tasks = store.optJSONArray("tasks");
        if (tasks == null) return out;
        long now = System.currentTimeMillis();
        for (int i = 0; i < tasks.length(); i++) {
            JSONObject t = tasks.optJSONObject(i);
            if (t == null || t.optBoolean("done") || t.optBoolean("reminder_fired") || t.isNull("reminder")) continue;
            String when = t.optString("reminder", null);
            long at = parse(when);
            // A little in the past too: one that came due while the phone was off.
            if (at > 0 && at > now - 24 * 3_600_000L) out.add(new Upcoming(t.optString("id"), at, when));
        }
        out.sort((a, b) -> Long.compare(a.at, b.at));
        return out.size() > MAX_ALARMS ? out.subList(0, MAX_ALARMS) : out;
    }

    static boolean hasUpcoming(Context c) {
        return !upcoming(c).isEmpty();
    }

    private static PendingIntent alarm(Context c, String id, String when) {
        Intent i = new Intent(c, Receiver.class).setAction("io.github.metman2013.rustdo.REMINDER").putExtra("id", id).putExtra("when", when);
        // One alarm per task: the data URI makes each PendingIntent distinct.
        i.setData(android.net.Uri.parse("rustdo://reminder/" + id));
        return PendingIntent.getBroadcast(c, 0, i, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    static synchronized void reschedule(Context c) {
        AlarmManager am = c.getSystemService(AlarmManager.class);
        if (am == null) return;
        SharedPreferences prefs = c.getSharedPreferences("rustdo", Context.MODE_PRIVATE);
        for (String id : prefs.getStringSet("alarms", new HashSet<>())) {
            am.cancel(alarm(c, id, ""));
        }
        Set<String> scheduled = new HashSet<>();
        boolean exact = Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms();
        long now = System.currentTimeMillis();
        for (Upcoming u : upcoming(c)) {
            long at = Math.max(u.at, now + 1000);
            PendingIntent pi = alarm(c, u.id, u.when);
            if (exact) {
                am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi);
            } else {
                // Without the "Alarms & reminders" permission Android may run it a few minutes late.
                am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi);
            }
            scheduled.add(u.id);
        }
        prefs.edit().putStringSet("alarms", scheduled).apply();
    }

    static void ensureChannel(Context c) {
        if (Build.VERSION.SDK_INT < 26) return;
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        if (nm == null || nm.getNotificationChannel(CHANNEL) != null) return;
        NotificationChannel ch = new NotificationChannel(CHANNEL, c.getString(R.string.reminders), NotificationManager.IMPORTANCE_HIGH);
        nm.createNotificationChannel(ch);
    }

    /** An alarm went off. */
    public static class Receiver extends BroadcastReceiver {
        @Override
        public void onReceive(Context c, Intent intent) {
            String id = intent.getStringExtra("id");
            String when = intent.getStringExtra("when");
            if (id == null) return;
            PendingResult result = goAsync();
            new Thread(() -> {
                try {
                    show(c, id, when);
                } finally {
                    result.finish();
                }
            }).start();
        }
    }

    private static void show(Context c, String id, String when) {
        // On screen, RustDo shows the reminder itself.
        if (MainActivity.resumed) return;
        JSONObject store = TaskStore.load(c);
        JSONObject t = store == null ? null : TaskStore.findTask(store, id);
        if (t == null || t.optBoolean("done") || t.optBoolean("reminder_fired")) return;
        // Changed since the alarm was set: the new time has its own alarm.
        if (when != null && !when.equals(t.optString("reminder"))) return;

        ensureChannel(c);
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        if (nm == null) return;
        Intent open = new Intent(c, MainActivity.class).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent tap = PendingIntent.getActivity(c, id.hashCode(), open, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification.Builder b = Build.VERSION.SDK_INT >= 26 ? new Notification.Builder(c, CHANNEL) : new Notification.Builder(c);
        b.setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(t.optString("title"))
                .setContentText(c.getString(R.string.reminder))
                .setContentIntent(tap)
                .setAutoCancel(true)
                .setCategory(Notification.CATEGORY_REMINDER)
                .setWhen(System.currentTimeMillis());
        if (Build.VERSION.SDK_INT < 26) b.setPriority(Notification.PRIORITY_HIGH).setDefaults(Notification.DEFAULT_ALL);
        try {
            nm.notify(id.hashCode(), b.build());
        } catch (SecurityException e) {
            return; // no permission to notify
        }
        TaskStore.reminderFired(c, id);
    }

    /** The phone restarted, or RustDo was updated: alarms are gone, set them again. */
    public static class Restore extends BroadcastReceiver {
        @Override
        public void onReceive(Context c, Intent intent) {
            PendingResult result = goAsync();
            new Thread(() -> {
                try {
                    reschedule(c);
                    TasksWidget.refreshAll(c);
                } finally {
                    result.finish();
                }
            }).start();
        }
    }
}
