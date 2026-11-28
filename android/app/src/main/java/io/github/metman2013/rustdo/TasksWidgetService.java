package io.github.metman2013.rustdo;

import android.appwidget.AppWidgetManager;
import android.content.Context;
import android.content.Intent;
import android.widget.RemoteViews;
import android.widget.RemoteViewsService;

import org.json.JSONArray;
import org.json.JSONObject;

import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Calendar;
import java.util.Date;
import java.util.List;
import java.util.Locale;

/** The widget's rows: the open tasks of its view, newest first. */
public class TasksWidgetService extends RemoteViewsService {
    @Override
    public RemoteViewsFactory onGetViewFactory(Intent intent) {
        return new Factory(getApplicationContext(), intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, 0));
    }

    private static final class Row {
        final String id;
        final String title;
        final String due;
        final boolean overdue;

        Row(String id, String title, String due, boolean overdue) {
            this.id = id;
            this.title = title;
            this.due = due;
            this.overdue = overdue;
        }
    }

    private static final class Factory implements RemoteViewsFactory {
        private final Context c;
        private final int widgetId;
        private List<Row> rows = new ArrayList<>();

        Factory(Context c, int widgetId) {
            this.c = c;
            this.widgetId = widgetId;
        }

        @Override
        public void onCreate() {}

        @Override
        public void onDestroy() {}

        @Override
        public void onDataSetChanged() {
            List<Row> out = new ArrayList<>();
            List<JSONObject> tasks = new ArrayList<>();
            JSONObject store = TaskStore.load(c);
            JSONArray all = store == null ? null : store.optJSONArray("tasks");
            String view = TasksWidget.view(c, widgetId);
            String today = TaskStore.today();
            if (all != null) {
                for (int i = 0; i < all.length(); i++) {
                    JSONObject t = all.optJSONObject(i);
                    if (t == null) continue;
                    Boolean pending = TaskStore.pendingDone(t.optString("id"));
                    boolean done = pending != null ? pending : t.optBoolean("done");
                    if (done) continue;
                    boolean in;
                    if (TasksWidget.MY_DAY.equals(view)) {
                        in = today.equals(t.optString("my_day"));
                    } else if (TasksWidget.IMPORTANT.equals(view)) {
                        in = t.optBoolean("important");
                    } else {
                        in = view.equals("list:" + t.optString("list_id"));
                    }
                    if (in) tasks.add(t);
                }
            }
            tasks.sort((a, b) -> b.optString("created").compareTo(a.optString("created")));
            for (JSONObject t : tasks) {
                String due = t.isNull("due") ? null : t.optString("due", null);
                out.add(new Row(t.optString("id"), t.optString("title"), due == null ? null : dueText(due, today), due != null && due.compareTo(today) < 0));
            }
            rows = out;
        }

        /** "Today", "Tomorrow", or a short date. */
        private String dueText(String due, String today) {
            if (due.equals(today)) return c.getString(R.string.today);
            Calendar t = Calendar.getInstance();
            t.add(Calendar.DAY_OF_MONTH, 1);
            if (due.equals(new SimpleDateFormat("yyyy-MM-dd", Locale.ROOT).format(t.getTime()))) return c.getString(R.string.tomorrow);
            try {
                Date d = new SimpleDateFormat("yyyy-MM-dd", Locale.ROOT).parse(due);
                return android.text.format.DateFormat.getMediumDateFormat(c).format(d);
            } catch (java.text.ParseException e) {
                return due;
            }
        }

        @Override
        public int getCount() {
            return rows.size();
        }

        @Override
        public RemoteViews getViewAt(int position) {
            RemoteViews rv = new RemoteViews(c.getPackageName(), R.layout.widget_row);
            if (position >= rows.size()) return rv;
            Row r = rows.get(position);
            rv.setTextViewText(R.id.row_title, r.title);
            if (r.due != null) {
                rv.setTextViewText(R.id.row_due, r.due);
                rv.setTextColor(R.id.row_due, r.overdue ? 0xFFDC2626 : c.getColor(R.color.widget_weak));
                rv.setViewVisibility(R.id.row_due, android.view.View.VISIBLE);
            } else {
                rv.setViewVisibility(R.id.row_due, android.view.View.GONE);
            }
            rv.setContentDescription(R.id.row_tick, c.getString(R.string.mark_done) + ": " + r.title);
            rv.setOnClickFillInIntent(R.id.row_tick, new Intent().putExtra("task", r.id));
            return rv;
        }

        @Override
        public RemoteViews getLoadingView() {
            return null;
        }

        @Override
        public int getViewTypeCount() {
            return 1;
        }

        @Override
        public long getItemId(int position) {
            return position < rows.size() ? rows.get(position).id.hashCode() : position;
        }

        @Override
        public boolean hasStableIds() {
            return true;
        }
    }
}
