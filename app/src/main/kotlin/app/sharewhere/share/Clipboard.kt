package app.sharewhere.share

import android.content.ClipData
import android.content.ClipDescription
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import android.widget.Toast

/**
 * Clipboard handling, with the platform quirks that bite in practice.
 */
object Clipboard {

    /** `ClipDescription.EXTRA_IS_SENSITIVE`, which is only a constant from API 33. */
    private const val EXTRA_IS_SENSITIVE = "android.content.extra.IS_SENSITIVE"

    /**
     * Copy [text] to the clipboard.
     *
     * @param sensitive masks the value in the system clipboard preview. Set it
     *   for locations. Deliberately *not* set for cleaned URLs: seeing the
     *   result is how the user knows the app did something.
     */
    fun copy(context: Context, label: String, text: String, sensitive: Boolean = false) {
        val clip = ClipData.newPlainText(label, text)
        if (sensitive) {
            clip.description.extras = PersistableBundle().apply {
                putBoolean(EXTRA_IS_SENSITIVE, true)
            }
        }
        context.clipboard.setPrimaryClip(clip)
    }

    /**
     * Confirm a copy, without doubling up on the system's own UI.
     *
     * API 33+ shows a clipboard preview automatically. Adding our own toast on
     * top of it gives the user two notifications for one action.
     */
    fun confirmIfNeeded(context: Context, message: String) {
        if (Build.VERSION.SDK_INT <= Build.VERSION_CODES.S_V2) {
            Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
        }
    }

    private val Context.clipboard
        get() = getSystemService(Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
}

/*
 * Deliberately absent: any call to getPrimaryClip().
 *
 * On Android 12+ reading the clipboard raises a system toast saying
 * "ShareWhere pasted from your clipboard" -- precisely the wrong signal from an
 * app whose entire pitch is that it does not snoop. The manual-paste screen
 * relies on the standard long-press paste gesture into a TextField instead, so
 * the text arrives because the user put it there.
 */
