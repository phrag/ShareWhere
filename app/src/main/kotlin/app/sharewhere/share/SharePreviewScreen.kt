package app.sharewhere.share

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uniffi.sharewhere.GeoLink
import uniffi.sharewhere.Outcome
import uniffi.sharewhere.RemovalKind
import uniffi.sharewhere.RemovedParam
import uniffi.sharewhere.Sanitized
import uniffi.sharewhere.defaultOptions
import uniffi.sharewhere.renderAllText
import uniffi.sharewhere.renderLinks

/**
 * The "here's what we removed" sheet.
 *
 * Showing the removed parameters rather than just asserting the link is clean
 * is the difference between a tool you trust and one you hope about. It is also
 * the only way a user can spot us stripping something load-bearing, which is
 * why "Copy original instead" is always available.
 */
@Composable
fun SharePreviewScreen(
    original: String,
    outcome: Outcome,
    networkAvailable: Boolean,
    resolving: Boolean,
    onResolve: (url: String) -> Unit,
    onCopy: (text: String, sensitive: Boolean) -> Unit,
    onShare: (text: String) -> Unit,
    onDismiss: () -> Unit,
) {
    Card(modifier = Modifier.fillMaxWidth().padding(16.dp)) {
        Column(
            modifier = Modifier.padding(20.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            when (outcome) {
                is Outcome.Text -> CleanedLink(outcome, original, onCopy, onShare)
                is Outcome.Location -> Location(outcome, onCopy, onShare)
                is Outcome.NeedsConsent -> NeedsConsent(
                    outcome = outcome,
                    networkAvailable = networkAvailable,
                    resolving = resolving,
                    onResolve = onResolve,
                )
                is Outcome.Unsupported -> Unsupported(outcome)
                Outcome.Nothing -> Text(
                    "Nothing to clean here.",
                    style = MaterialTheme.typography.bodyMedium,
                )
            }

            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Close")
            }
        }
    }
}

@Composable
private fun CleanedLink(
    outcome: Outcome.Text,
    original: String,
    onCopy: (String, Boolean) -> Unit,
    onShare: (String) -> Unit,
) {
    val tracker = outcome.urls.firstOrNull { it.completeProvider }
    if (tracker != null) {
        Text("This whole link is a tracker", style = MaterialTheme.typography.titleMedium)
        Text(
            "There is no destination to clean out of it. Opening it only tells " +
                "${tracker.providers.firstOrNull() ?: "the tracker"} that you did.",
            style = MaterialTheme.typography.bodySmall,
        )
    } else {
        Text(
            if (outcome.changed) "Cleaned" else "Already clean",
            style = MaterialTheme.typography.titleMedium,
        )
    }

    Text(
        outcome.cleaned,
        style = MaterialTheme.typography.bodyMedium,
        maxLines = 4,
        overflow = TextOverflow.Ellipsis,
    )

    val removed = outcome.urls.flatMap(Sanitized::removed)
    if (removed.isNotEmpty()) {
        Text("Removed ${removed.size}", style = MaterialTheme.typography.labelLarge)
        removed.take(12).forEach { RemovedRow(it) }
        if (removed.size > 12) {
            Text("and ${removed.size - 12} more", style = MaterialTheme.typography.labelSmall)
        }
    }

    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(onClick = { onCopy(outcome.cleaned, false) }) { Text("Copy") }
        Button(onClick = { onShare(outcome.cleaned) }) { Text("Share…") }
    }

    // The escape hatch. If we broke the link, the user needs a way out that is
    // not "go back to the other app and copy it again".
    if (outcome.changed) {
        TextButton(onClick = { onCopy(original, false) }) {
            Text("Copy original instead")
        }
    }
}

@Composable
private fun RemovedRow(param: RemovedParam) {
    val kind = when (param.kind) {
        RemovalKind.TRACKING -> "tracking"
        RemovalKind.REFERRAL -> "referral"
        RemovalKind.REWRITE -> "rewrite"
    }
    Text(
        "· ${param.key} ($kind, ${param.provider})",
        style = MaterialTheme.typography.bodySmall,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
private fun Location(
    outcome: Outcome.Location,
    onCopy: (String, Boolean) -> Unit,
    onShare: (String) -> Unit,
) {
    val links: List<GeoLink> = renderLinks(outcome.point, defaultOptions())

    Text("Location", style = MaterialTheme.typography.titleMedium)
    outcome.point.label?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }

    links.forEach { link ->
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column(modifier = Modifier.padding(end = 8.dp)) {
                Text(link.label, style = MaterialTheme.typography.labelMedium)
                Text(
                    link.value,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            // Locations are marked sensitive so the system clipboard preview
            // masks them.
            TextButton(onClick = { onCopy(link.value, true) }) { Text("Copy") }
        }
    }

    val all = renderAllText(outcome.point, defaultOptions())
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(onClick = { onCopy(all, true) }) { Text("Copy all") }
        Button(onClick = { onShare(all) }) { Text("Share…") }
    }
}

@Composable
private fun NeedsConsent(
    outcome: Outcome.NeedsConsent,
    networkAvailable: Boolean,
    resolving: Boolean,
    onResolve: (String) -> Unit,
) {
    Text("This link hides where it points", style = MaterialTheme.typography.titleMedium)
    Text(outcome.explanation, style = MaterialTheme.typography.bodySmall)
    // The host is shown on its own line, deliberately: it is the single fact
    // the user is being asked to agree to contact.
    Text(outcome.host, style = MaterialTheme.typography.bodyMedium)

    if (networkAvailable) {
        // Consent is per link. There is deliberately no "always allow" here:
        // the whole point is that each request is a decision the user makes.
        Button(
            onClick = { onResolve(outcome.url) },
            enabled = !resolving,
        ) {
            Text(if (resolving) "Contacting ${outcome.host}…" else "Resolve this one link")
        }
    } else {
        Text(
            "This build has no internet permission at all, so it cannot follow " +
                "the link. Install the standard build if you want that option.",
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
private fun Unsupported(outcome: Outcome.Unsupported) {
    Text("Can't convert this offline", style = MaterialTheme.typography.titleMedium)
    Text(outcome.detail, style = MaterialTheme.typography.bodyMedium)
    Text(outcome.explanation, style = MaterialTheme.typography.bodySmall)
}
