package app.sharewhere.share

import android.app.Activity
import android.content.ComponentName
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
import androidx.lifecycle.lifecycleScope
import app.sharewhere.R
import app.sharewhere.net.ResolveCoordinator
import app.sharewhere.settings.Settings
import app.sharewhere.settings.SettingsRepository
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import uniffi.sharewhere.Outcome
import uniffi.sharewhere.ResolveSession
import uniffi.sharewhere.Step

/**
 * The share target.
 *
 * Two entry points, exposed as two aliases so both appear in the share sheet:
 *
 *  - **Clean Copy** cleans, copies, and finishes without showing a screen —
 *    with a brief popup naming what it removed, so "instant" does not mean
 *    "silent about what happened".
 *  - **Clean Share** shows exactly what was removed, then re-shares.
 *
 * The preview is forced regardless of entry point when finishing quietly would
 * be misleading: the link needs the network, the link is entirely a tracker, or
 * the input is a location with several formats to choose between.
 */
class ShareActivity : ComponentActivity() {

    private enum class Mode { CopyOnly, Preview }

    private val coordinator = ResolveCoordinator()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val input = extractInput(intent)
        if (input.isNullOrBlank()) {
            finish()
            return
        }

        // An activity-alias cannot carry an extra, so the entry the user picked
        // is identified by which alias the system launched.
        val mode = when (intent.component?.className) {
            ALIAS_CLEAN_AND_SHARE -> Mode.Preview
            else -> Mode.CopyOnly
        }

        lifecycleScope.launch {
            val settings = SettingsRepository(applicationContext).settings.first()
            handle(input, mode, settings)
        }
    }

    private fun handle(input: String, mode: Mode, settings: Settings) {
        // Offline by default: the options carry allowNetwork = false, so the
        // core returns NeedsConsent rather than reaching for the network.
        val session = ResolveSession(input, settings.toOptions())

        when (val step = session.advance()) {
            is Step.Done -> present(input, step.outcome, mode, settings)
            // Unreachable with allowNetwork = false; the core only emits a
            // Fetch once consent has been granted for a specific resolve.
            is Step.Fetch -> present(input, Outcome.Nothing, Mode.Preview, settings)
        }
    }

    private fun present(original: String, outcome: Outcome, mode: Mode, settings: Settings) {
        // PROCESS_TEXT is a special case: the caller wants replacement text
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

        if (mode == Mode.CopyOnly && !settings.alwaysPreview && !forcePreview &&
            outcome is Outcome.Text
        ) {
            Clipboard.copy(this, getString(R.string.clip_label_link), outcome.cleaned)
            // Always shown, unlike a "copied!" confirmation: this says what was
            // taken out, which the system clipboard preview does not.
            Clipboard.announce(this, removalSummary(outcome))
            finish()
            return
        }

        setContent {
            MaterialTheme {
                var current by remember { mutableStateOf(outcome) }
                var resolving by remember { mutableStateOf(false) }
                val scope = rememberCoroutineScope()

                SharePreviewScreen(
                    original = original,
                    outcome = current,
                    resolutionOffered = settings.offerLinkResolution,
                    resolving = resolving,
                    onResolve = { url ->
                        scope.launch {
                            resolving = true
                            // Consent is granted for this one call. Stored
                            // settings are untouched, so the next share starts
                            // offline again.
                            current = runCatching {
                                coordinator.resolve(
                                    url,
                                    settings.toOptions().copy(allowNetwork = true),
                                )
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

    /** "Removed 3 trackers", or an honest note that there was nothing to remove. */
    private fun removalSummary(outcome: Outcome.Text): String {
        val removed = outcome.urls.sumOf { it.removed.size }
        return when {
            removed == 0 -> getString(R.string.copied_unchanged)
            removed == 1 -> getString(R.string.copied_one_removed)
            else -> getString(R.string.copied_many_removed, removed)
        }
    }

    /** Hand the cleaned text back to whichever text field invoked us. */
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
            // Without this we appear in our own chooser, which makes it easy to
            // loop back into ShareWhere by mistake.
            putExtra(
                Intent.EXTRA_EXCLUDE_COMPONENTS,
                arrayOf(ComponentName(this@ShareActivity, ShareActivity::class.java)),
            )
        }
        startActivity(chooser)
        finish()
    }

    private fun extractInput(intent: Intent): String? = when (intent.action) {
        Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)
        Intent.ACTION_PROCESS_TEXT ->
            intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString()
        Intent.ACTION_VIEW -> intent.dataString
        else -> null
    }

    private companion object {
        /** Must match the `activity-alias` names in the manifest. */
        const val ALIAS_CLEAN_AND_SHARE = "app.sharewhere.share.CleanAndShare"
    }
}
