package io.github.metman2013.rustdo;

import android.content.Context;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.text.SimpleDateFormat;
import java.util.Calendar;
import java.util.Date;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;

/**
 * RustDo's task file (tasks.json in the app's files folder, written by the Rust app), for
 * the widget and reminders.
 *
 * While RustDo's own process is running (MainActivity alive) the Rust app owns the file:
 * changes are handed to it through card-actions.jsonl (see src/pinned.rs), never written
 * here. Otherwise this edits the file directly, the way the Rust app would; every field it
 * doesn't know is kept as it is.
 */
final class TaskStore {
    private TaskStore() {}

    /** Ticks done here that the running app hasn't saved yet: task id -> done. */
    private static final Map<String, Boolean> pending = new HashMap<>();

    static File file(Context c) {
        return new File(c.getFilesDir(), "tasks.json");
    }

    static JSONObject load(Context c) {
        try {
            byte[] bytes = Files.readAllBytes(file(c).toPath());
            return new JSONObject(new String(bytes, StandardCharsets.UTF_8));
        } catch (IOException | JSONException e) {
            return null;
        }
    }

    private static void save(Context c, JSONObject store) throws IOException, JSONException {
        File f = file(c);
        File tmp = new File(f.getPath() + ".tmp");
        try (FileOutputStream out = new FileOutputStream(tmp)) {
            out.write(store.toString(2).getBytes(StandardCharsets.UTF_8));
            out.getFD().sync();
        }
        if (!tmp.renameTo(f)) throw new IOException("couldn't replace " + f);
    }

    /** Local time as RustDo writes it: 2026-11-28T17:03:06.723 */
    static String now() {
        return new SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS", Locale.ROOT).format(new Date());
    }

    static String today() {
        return new SimpleDateFormat("yyyy-MM-dd", Locale.ROOT).format(new Date());
    }

    static synchronized Boolean pendingDone(String taskId) {
        return pending.get(taskId);
    }

    /** The app saved: what it was handed is in the file now. */
    static synchronized void clearPending() {
        pending.clear();
    }

    /** Appends one action for the running app (a line of src/pinned.rs's CardAction). */
    private static void handoff(Context c, JSONObject action) throws IOException {
        try (FileOutputStream out = new FileOutputStream(new File(c.getFilesDir(), "card-actions.jsonl"), true)) {
            out.write((action.toString() + "\n").getBytes(StandardCharsets.UTF_8));
        }
    }

    static JSONObject findTask(JSONObject store, String id) {
        JSONArray tasks = store.optJSONArray("tasks");
        if (tasks == null) return null;
        for (int i = 0; i < tasks.length(); i++) {
            JSONObject t = tasks.optJSONObject(i);
            if (t != null && id.equals(t.optString("id"))) return t;
        }
        return null;
    }

    /** Ticks a task off (or back on), from the widget. */
    static synchronized void setDone(Context c, String taskId, boolean done) {
        try {
            if (MainActivity.alive) {
                JSONObject a = new JSONObject();
                a.put("SetDone", new JSONArray().put(taskId).put(done));
                handoff(c, a);
                pending.put(taskId, done);
                return;
            }
            JSONObject store = load(c);
            if (store == null) return;
            JSONArray tasks = store.getJSONArray("tasks");
            for (int i = 0; i < tasks.length(); i++) {
                JSONObject t = tasks.getJSONObject(i);
                if (!taskId.equals(t.optString("id"))) continue;
                if (t.optBoolean("done") == done) return;
                t.put("done", done);
                t.put("completed_at", done ? now() : JSONObject.NULL);
                if (done) {
                    JSONObject next = nextOccurrence(t);
                    if (next != null) {
                        // The completed one stops repeating, as in RustDo (Store::set_done).
                        t.put("recurrence", "Never");
                        insert(tasks, i, next);
                    }
                }
                save(c, store);
                return;
            }
        } catch (IOException | JSONException e) {
            android.util.Log.w("RustDo", "widget: couldn't tick off a task", e);
        }
    }

    /** A reminder was shown as a notification: don't show it again. */
    static synchronized void reminderFired(Context c, String taskId) {
        try {
            if (MainActivity.alive) {
                JSONObject a = new JSONObject();
                a.put("ReminderFired", taskId);
                handoff(c, a);
                return;
            }
            JSONObject store = load(c);
            if (store == null) return;
            JSONObject t = findTask(store, taskId);
            if (t == null || t.optBoolean("reminder_fired")) return;
            t.put("reminder_fired", true);
            save(c, store);
        } catch (IOException | JSONException e) {
            android.util.Log.w("RustDo", "reminders: couldn't mark a reminder as shown", e);
        }
    }

    private static void insert(JSONArray arr, int index, JSONObject item) throws JSONException {
        for (int j = arr.length(); j > index; j--) arr.put(j, arr.get(j - 1));
        arr.put(index, item);
    }

    // ------------------------------------------------------------------ repeats

    /** RustDo's Recurrence::next_after_every; null when it doesn't repeat. */
    static Calendar nextDate(String recurrence, Calendar from, int every) {
        int n = Math.max(every, 1);
        Calendar d = (Calendar) from.clone();
        switch (recurrence) {
            case "Daily": d.add(Calendar.DAY_OF_MONTH, n); return d;
            case "Weekly": d.add(Calendar.DAY_OF_MONTH, 7 * n); return d;
            case "Monthly": d.add(Calendar.MONTH, n); return d;
            case "Yearly": d.add(Calendar.YEAR, n); return d;
            case "Weekdays":
                do { d.add(Calendar.DAY_OF_MONTH, 1); }
                while (d.get(Calendar.DAY_OF_WEEK) == Calendar.SATURDAY || d.get(Calendar.DAY_OF_WEEK) == Calendar.SUNDAY);
                return d;
            default: return null;
        }
    }

    private static Calendar parseDate(String s) {
        if (s == null || s.length() < 10) return null;
        try {
            Calendar c = Calendar.getInstance();
            c.clear();
            c.set(Integer.parseInt(s.substring(0, 4)), Integer.parseInt(s.substring(5, 7)) - 1, Integer.parseInt(s.substring(8, 10)));
            return c;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static String formatDate(Calendar c) {
        return String.format(Locale.ROOT, "%04d-%02d-%02d", c.get(Calendar.YEAR), c.get(Calendar.MONTH) + 1, c.get(Calendar.DAY_OF_MONTH));
    }

    /** The next one of a repeating task that was just completed (Store::set_done). */
    static JSONObject nextOccurrence(JSONObject t) throws JSONException {
        String recurrence = t.optString("recurrence", "Never");
        if (recurrence.equals("Never")) return null;
        String due = t.isNull("due") ? null : t.optString("due", null);
        String reminder = t.isNull("reminder") ? null : t.optString("reminder", null);
        Calendar base = parseDate(due != null ? due : reminder);
        if (base == null) base = parseDate(today());
        Calendar next = nextDate(recurrence, base, t.optInt("repeat_every", 1));
        if (next == null) return null;
        long shiftDays = Math.round((next.getTimeInMillis() - base.getTimeInMillis()) / 86_400_000.0);

        JSONObject n = new JSONObject(t.toString());
        n.put("id", UUID.randomUUID().toString());
        n.put("done", false);
        n.put("completed_at", JSONObject.NULL);
        n.put("created", now());
        n.put("my_day", JSONObject.NULL);
        n.put("reminder_fired", false);
        if (due != null) n.put("due", formatDate(next));
        if (reminder != null && reminder.length() >= 10) {
            Calendar r = parseDate(reminder);
            if (r != null) {
                r.add(Calendar.DAY_OF_MONTH, (int) shiftDays);
                n.put("reminder", formatDate(r) + reminder.substring(10));
            }
        }
        JSONArray steps = n.optJSONArray("steps");
        if (steps != null) {
            for (int i = 0; i < steps.length(); i++) {
                JSONObject s = steps.optJSONObject(i);
                if (s != null) {
                    s.put("id", UUID.randomUUID().toString());
                    s.put("done", false);
                }
            }
        }
        return n;
    }
}
