package app.sharewhere.share

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.lifecycle.lifecycleScope
import app.sharewhere.BuildConfig
import app.sharewhere.R
import kotlinx.coroutines.launch
import uniffi.sharewhere.Outcome
import uniffi.sharewhere.ResolveSession
import uniffi.sharewhere.Step
import uniffi.sharewhere.defaultOptions

/**
 * The share target.
 *
 * Two entry points, exposed as two share shortcuts:
 *
 *  - [Mode.CopyOnly] cleans, copies and finishes without showing a screen.
 *  - [Mode.Preview] shows exactly what was removed, then offers a re-share.
 *
 * The preview is forced regardless of mode in three cases, because finishing
 * silently would be misleading: the link needs the network, the link is
 * entirely a tracker, or the input is a location with several formats to pick
 * from.
 */
class ShareActivity : ComponentActivity() {

    private enum class Mode { CopyOnly, Preview }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val input = extractInput(intent)
        if (input.isNullOrBlank()) {
            finish()
            return
        }

        val mode = when (intent.getStringExtra(EXTRA_MODE)) {
            MODE_PREVIEW -> Mode.Preview
            else -> Mode.CopyOnly
        }

        lifecycleScope.launch { handle(input, mode) }
    }

    private suspend fun handle(input: String, mode: Mode) {
        // Offline by default: allowNetwork stays false, so the core returns
        // NeedsConsent rather than reaching for the network on its own.
        val session = ResolveSession(input, defaultOptions())

        when (val step = session.advance()) {
            is Step.Done -> present(input, step.outcome, mode)
            is Step.Fetch -> {
                // Unreachable with the default options; the core only emits a
                // Fetch once allowNetwork has been set for this one resolve.
                present(input, Outcome.Nothing, Mode.Preview)
            }
        }
    }

    private fun present(original: String, outcome: Outcome, mode: Mode) {
        // PROCESS_TEXT is a special case: the caller wants the replacement text
        // back, not a clipboard write or a chooser.
        if (intent.action == Intent.ACTION_PROCESS_TEXT) {
            returnProcessedText(outcome)
            return
        }

        val forcePreview = when (outcome) {
            is Outcome.NeedsConsent -> true      // never fetch without asking
            is Outcome.Location -> true          // several formats to choose from
            is Outcome.Unsupported -> true       // needs an explanation
            is Outcome.Text -> outcome.urls.any { it.completeProvider }
            Outcome.Nothing -> true
        }

        if (mode == Mode.CopyOnly && !forcePreview && outcome is Outcome.Text) {
            Clipboard.copy(this, getString(R.string.clip_label_link), outcome.cleaned)
            Clipboard.confirmIfNeeded(
                this,
                if (outcome.changed) {
                    getString(R.string.copied_cleaned)
                } else {
                    getString(R.string.copied_unchanged)
                },
            )
            finish()
            return
        }

        setContent {
            SharePreviewScreen(
                original = original,
                outcome = outcome,
                networkAvailable = BuildConfig.NETWORK_AVAILABLE,
                onCopy = { text, sensitive ->
                    Clipboard.copy(this, getString(R.string.clip_label_link), text, sensitive)
                },
                onShare = { text -> reshare(text) },
                onDismiss = { finish() },
            )
        }
    }

    /**
     * Hand the cleaned text straight back to whichever text field invoked us.
     */
    private fun returnProcessedText(outcome: Outcome) {
        val replacement = when (outcome) {
            is Outcome.Text -> outcome.cleaned
            // Anything we could not clean is returned untouched. Mangling text
            // we did not understand would be worse than doing nothing.
            else -> intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString()
        }
        if (replacement != null) {
            setResult(
                Activity.RESULT_OK,
                Intent().putExtra(Intent.EXTRA_PROCESS_TEXT, replacement),
            )
        }
        finish()
    }

    private fun reshare(text: String) {
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, text)
        }
        val chooser = Intent.createChooser(send, getString(R.string.share_cleaned)).apply {
            // Without this we appear in our own chooser, which is confusing and
            // makes it easy to loop.
            putExtra(
                Intent.EXTRA_EXCLUDE_COMPONENTS,
                arrayOf(android.content.ComponentName(this@ShareActivity, ShareActivity::class.java)),
            )
        }
        startActivity(chooser)
        finish()
    }

    private fun extractInput(intent: Intent): String? = when (intent.action) {
        Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)
        Intent.ACTION_PROCESS_TEXT -> intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString()
        Intent.ACTION_VIEW -> intent.dataString
        else -> null
    }

    companion object {
        const val EXTRA_MODE = "app.sharewhere.MODE"
        const val MODE_PREVIEW = "preview"
        const val MODE_COPY = "copy"
    }
}
