package io.github.metman2013.rustdo;

import android.app.Activity;
import android.appwidget.AppWidgetManager;
import android.content.Intent;
import android.os.Bundle;
import android.widget.ArrayAdapter;
import android.widget.ListView;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.List;

/** "What should the widget show?": My Day, Important, or one of your lists. */
public class WidgetConfig extends Activity {
    private int widgetId;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setResult(RESULT_CANCELED);
        widgetId = getIntent().getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID);
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) {
            finish();
            return;
        }
        setTitle(R.string.widget_choose);

        List<String> labels = new ArrayList<>();
        List<String> views = new ArrayList<>();
        labels.add("☀  " + getString(R.string.my_day));
        views.add(TasksWidget.MY_DAY);
        labels.add("★  " + getString(R.string.important));
        views.add(TasksWidget.IMPORTANT);
        JSONObject store = TaskStore.load(this);
        JSONArray lists = store == null ? null : store.optJSONArray("lists");
        if (lists != null) {
            for (int i = 0; i < lists.length(); i++) {
                JSONObject l = lists.optJSONObject(i);
                if (l == null) continue;
                labels.add("📋  " + l.optString("name"));
                views.add("list:" + l.optString("id"));
            }
        }

        ListView list = new ListView(this);
        list.setAdapter(new ArrayAdapter<>(this, android.R.layout.simple_list_item_1, labels));
        list.setOnItemClickListener((parent, v, position, id) -> {
            TasksWidget.setView(this, widgetId, views.get(position));
            TasksWidget.refreshAll(this);
            setResult(RESULT_OK, new Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId));
            finish();
        });
        setContentView(list);
    }
}
