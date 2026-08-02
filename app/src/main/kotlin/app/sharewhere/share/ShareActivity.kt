package app.sharewhere.share

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import app.sharewhere.net.createLinkResolver
import androidx.lifecycle.lifecycleScope
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

    /** Offline in the `offline` flavor, OkHttp-backed in `standard`. */
    private val resolver by lazy { createLinkResolver() }

    /**
     * The stored options with the network permitted, for a single resolve the
     * user has just tapped to allow. Deliberately not persisted: there is no
     * "always allow" in v1.
     */
    private fun consentedOptions() = defaultOptions().copy(allowNetwork = true)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val input = extractInput(intent)
        if (input.isNullOrBlank()) {
            finish()
            return
        }

        // An activity-alias cannot carry an extra, so the entry the user picked
        // in the share sheet is identified by which alias the system launched.
        val mode = when (intent.component?.className) {
            ALIAS_CLEAN_AND_SHARE -> Mode.Preview
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
            // Material3 components read their colours and typography from this;
            // without it they fall back to bare defaults and look wrong.
            MaterialTheme {
                // The outcome is state, not a constant: resolving a short link
                // replaces it with the location that was behind it.
                var current by remember { mutableStateOf(outcome) }
                var resolving by remember { mutableStateOf(false) }
                val scope = rememberCoroutineScope()

                SharePreviewScreen(
                    original = original,
                    outcome = current,
                    networkAvailable = resolver.available,
                    resolving = resolving,
                    onResolve = { url ->
                        scope.launch {
                            resolving = true
                            // Consent is granted for this one call. The stored
                            // options are untouched, so the next share starts
                            // offline again.
                            current = runCatching {
                                resolver.resolve(url, consentedOptions())
                            }.getOrElse { Outcome.Nothing }
                            resolving = false
                        }
                    },
                    onCopy = { text, sensitive ->
                        Clipboard.copy(this, getString(R.string.clip_label_link), text, sensitive)
                    },
                    onShare = { text -> reshare(text) },
                    onDismiss = { finish() },
                )
            }
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
        /** Must match the `activity-alias` names in the manifest. */
        const val ALIAS_CLEAN_AND_COPY = "app.sharewhere.share.CleanAndCopy"
        const val ALIAS_CLEAN_AND_SHARE = "app.sharewhere.share.CleanAndShare"
    }
}
