package io.github.metman2013.rustdo;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.widget.RemoteViews;

import org.json.JSONArray;
import org.json.JSONObject;

/**
 * The home-screen (and, where Android allows it, lock-screen) widget: My Day, Important or
 * one list, with a tick for each task. Tapping the title opens RustDo; "+" opens it to add.
 */
public class TasksWidget extends AppWidgetProvider {
    static final String ACTION_TICK = "io.github.metman2013.rustdo.TICK";
    static final String MY_DAY = "myday";
    static final String IMPORTANT = "important";

    static String view(Context c, int widgetId) {
        return prefs(c).getString("widget_" + widgetId, MY_DAY);
    }

    static void setView(Context c, int widgetId, String view) {
        prefs(c).edit().putString("widget_" + widgetId, view).apply();
    }

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences("rustdo", Context.MODE_PRIVATE);
    }

    /** The widget's heading: "My Day", "Important", or the list's name. */
    static String title(Context c, String view, JSONObject store) {
        if (MY_DAY.equals(view)) return c.getString(R.string.my_day);
        if (IMPORTANT.equals(view)) return c.getString(R.string.important);
        JSONArray lists = store == null ? null : store.optJSONArray("lists");
        if (lists != null) {
            for (int i = 0; i < lists.length(); i++) {
                JSONObject l = lists.optJSONObject(i);
                if (l != null && view.equals("list:" + l.optString("id"))) return l.optString("name");
            }
        }
        return c.getString(R.string.app_name);
    }

    static void refreshAll(Context c) {
        AppWidgetManager m = AppWidgetManager.getInstance(c);
        int[] ids = m.getAppWidgetIds(new ComponentName(c, TasksWidget.class));
        if (ids.length == 0) return;
        for (int id : ids) update(c, m, id);
        m.notifyAppWidgetViewDataChanged(ids, R.id.widget_list);
    }

    static void update(Context c, AppWidgetManager m, int widgetId) {
        String view = view(c, widgetId);
        RemoteViews rv = new RemoteViews(c.getPackageName(), R.layout.widget);
        rv.setTextViewText(R.id.widget_title, title(c, view, TaskStore.load(c)));

        Intent open = new Intent(c, MainActivity.class).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent openPi = PendingIntent.getActivity(c, 0, open, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        rv.setOnClickPendingIntent(R.id.widget_title, openPi);
        rv.setOnClickPendingIntent(R.id.widget_add, openPi);

        Intent svc = new Intent(c, TasksWidgetService.class).putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId);
        svc.setData(Uri.parse(svc.toUri(Intent.URI_INTENT_SCHEME)));
        rv.setRemoteAdapter(R.id.widget_list, svc);
        rv.setEmptyView(R.id.widget_list, R.id.widget_empty);

        // Rows fill in the task id; the template says what to do with it.
        Intent tick = new Intent(c, TasksWidget.class).setAction(ACTION_TICK);
        PendingIntent tickPi = PendingIntent.getBroadcast(c, widgetId, tick, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_MUTABLE);
        rv.setPendingIntentTemplate(R.id.widget_list, tickPi);
        m.updateAppWidget(widgetId, rv);
    }

    @Override
    public void onUpdate(Context c, AppWidgetManager m, int[] ids) {
        for (int id : ids) update(c, m, id);
        m.notifyAppWidgetViewDataChanged(ids, R.id.widget_list);
    }

    @Override
    public void onDeleted(Context c, int[] ids) {
        SharedPreferences.Editor e = prefs(c).edit();
        for (int id : ids) e.remove("widget_" + id);
        e.apply();
    }

    @Override
    public void onReceive(Context c, Intent intent) {
        if (ACTION_TICK.equals(intent.getAction())) {
            String id = intent.getStringExtra("task");
            if (id == null) return;
            PendingResult result = goAsync();
            new Thread(() -> {
                try {
                    TaskStore.setDone(c, id, true);
                    refreshAll(c);
                    Reminders.reschedule(c);
                } finally {
                    result.finish();
                }
            }).start();
            return;
        }
        super.onReceive(c, intent);
    }
}
