package io.github.metman2013.rustdo;

import android.Manifest;
import android.app.NativeActivity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Matrix;
import android.media.ExifInterface;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.MediaStore;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;

/**
 * RustDo's activity: the Rust app (librustdo.so, through NativeActivity), plus the few
 * things a NativeActivity can't do alone. The Rust side calls the public methods here
 * through JNI (src/platform.rs).
 */
public class MainActivity extends NativeActivity {
    /** RustDo's own process has the app running: it owns tasks.json (see TaskStore). */
    static volatile boolean alive;
    /** RustDo is on screen: it shows reminders itself. */
    static volatile boolean resumed;

    private static final int PICK_PHOTO = 4711;
    private static final int ASK_NOTIFICATIONS = 4712;
    /** Longest side of a picked photo, in pixels. */
    private static final int MAX_SIDE = 1920;

    // The photo picker's answer: 0 waiting, 1 a photo, 2 cancelled, 3 failed.
    private volatile int pickState = 2;
    private volatile byte[] picked;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        alive = true;
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onResume() {
        super.onResume();
        resumed = true;
    }

    @Override
    protected void onPause() {
        resumed = false;
        super.onPause();
        // Leaving RustDo: the widget and the scheduled reminders catch up with any changes.
        new Thread(this::refresh).start();
    }

    @Override
    protected void onDestroy() {
        alive = false;
        super.onDestroy();
    }

    // ------------------------------------------------------------------ called from Rust

    /** The Rust app saved tasks.json. */
    public void dataChanged() {
        refresh();
        runOnUiThread(this::askForNotificationsOnce);
    }

    private void refresh() {
        TaskStore.clearPending();
        TasksWidget.refreshAll(this);
        Reminders.reschedule(this);
    }

    /** Android 13+: reminders need permission to notify. Asked once, when there are any. */
    private void askForNotificationsOnce() {
        if (Build.VERSION.SDK_INT < 33 || !Reminders.hasUpcoming(this)) return;
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return;
        SharedPreferences prefs = getSharedPreferences("rustdo", MODE_PRIVATE);
        if (prefs.getBoolean("asked_notifications", false)) return;
        prefs.edit().putBoolean("asked_notifications", true).apply();
        requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, ASK_NOTIFICATIONS);
    }

    /** Opens Android's photo picker (no permission needed). */
    public boolean pickPhoto() {
        pickState = 0;
        picked = null;
        runOnUiThread(() -> {
            Intent intent;
            if (Build.VERSION.SDK_INT >= 33) {
                intent = new Intent(MediaStore.ACTION_PICK_IMAGES);
            } else {
                intent = new Intent(Intent.ACTION_GET_CONTENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
            }
            intent.setType("image/*");
            try {
                startActivityForResult(intent, PICK_PHOTO);
            } catch (RuntimeException e) {
                pickState = 3;
            }
        });
        return true;
    }

    public int pickedPhotoState() {
        return pickState;
    }

    /** The picked photo as a JPEG, once. */
    public byte[] takePickedPhoto() {
        byte[] b = picked;
        picked = null;
        pickState = 2;
        return b;
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != PICK_PHOTO) return;
        Uri uri = data != null ? data.getData() : null;
        if (resultCode != RESULT_OK || uri == null) {
            pickState = 2;
            return;
        }
        new Thread(() -> {
            try {
                picked = shrink(uri);
                pickState = picked != null ? 1 : 3;
            } catch (IOException | RuntimeException e) {
                pickState = 3;
            }
        }).start();
    }

    /** Reads the photo at most MAX_SIDE pixels long, the right way up, as a JPEG. */
    private byte[] shrink(Uri uri) throws IOException {
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        try (InputStream in = getContentResolver().openInputStream(uri)) {
            BitmapFactory.decodeStream(in, null, bounds);
        }
        int sample = 1;
        while (Math.max(bounds.outWidth, bounds.outHeight) / (sample * 2) >= MAX_SIDE) sample *= 2;
        BitmapFactory.Options opts = new BitmapFactory.Options();
        opts.inSampleSize = sample;
        Bitmap bmp;
        try (InputStream in = getContentResolver().openInputStream(uri)) {
            bmp = BitmapFactory.decodeStream(in, null, opts);
        }
        if (bmp == null) return null;
        int degrees = 0;
        try (InputStream in = getContentResolver().openInputStream(uri)) {
            int o = new ExifInterface(in).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL);
            degrees = o == ExifInterface.ORIENTATION_ROTATE_90 ? 90 : o == ExifInterface.ORIENTATION_ROTATE_180 ? 180 : o == ExifInterface.ORIENTATION_ROTATE_270 ? 270 : 0;
        } catch (IOException | RuntimeException ignored) {
            // No EXIF: leave it as it is.
        }
        float scale = Math.min(1f, (float) MAX_SIDE / Math.max(bmp.getWidth(), bmp.getHeight()));
        if (degrees != 0 || scale < 1f) {
            Matrix m = new Matrix();
            m.postScale(scale, scale);
            m.postRotate(degrees);
            bmp = Bitmap.createBitmap(bmp, 0, 0, bmp.getWidth(), bmp.getHeight(), m, true);
        }
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        bmp.compress(Bitmap.CompressFormat.JPEG, 88, out);
        return out.toByteArray();
    }
}
