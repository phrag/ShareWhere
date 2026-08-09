package app.sharewhere

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import app.sharewhere.settings.LinkHandlers
import app.sharewhere.settings.Settings
import app.sharewhere.settings.SettingsRepository
import kotlinx.coroutines.launch
import uniffi.sharewhere.Precision

/**
 * Home, settings and about.
 *
 * The share sheet is the real entry point, so this screen exists mostly to let
 * you change how ShareWhere behaves when you use it from there.
 */
class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val repository = SettingsRepository(applicationContext)

        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    val settings by repository.settings.collectAsState(initial = Settings())
                    val scope = rememberCoroutineScope()

                    SettingsScreen(
                        settings = settings,
                        onChange = { transform -> scope.launch { repository.update(transform) } },
                        onOpenProject = { openProject() },
                    )
                }
            }
        }
    }

    private fun openProject() {
        // The one place the app opens a URL. Nothing is fetched in-process.
        runCatching {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(BuildConfig.PROJECT_URL)))
        }
    }
}

@Composable
private fun SettingsScreen(
    settings: Settings,
    onChange: ((Settings) -> Settings) -> Unit,
    onOpenProject: () -> Unit,
) {
    val context = LocalContext.current
    var handlersEnabled by remember { mutableStateOf(LinkHandlers.enabled(context)) }

    Column(
        modifier = Modifier
            .padding(24.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text("ShareWhere", style = MaterialTheme.typography.headlineMedium)
        Text(
            "Share a link here and it comes back without the tracking. Share a " +
                "location and you get every map format at once.",
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.padding(top = 8.dp, bottom = 16.dp),
        )

        Section("Links")
        SettingSwitch(
            title = "Remove affiliate tags",
            subtitle = "Strips things like Amazon's tag=. Turning this off keeps " +
                "them, so a creator still gets their commission.",
            checked = settings.removeReferral,
            onCheckedChange = { value -> onChange { it.copy(removeReferral = value) } },
        )
        SettingSwitch(
            title = "Unwrap redirects",
            subtitle = "Follows links that only wrap a destination, like " +
                "google.com/url?q=, without going online.",
            checked = settings.followRedirects,
            onCheckedChange = { value -> onChange { it.copy(followRedirects = value) } },
        )
        SettingSwitch(
            title = "Offer to resolve short links",
            subtitle = "maps.app.goo.gl and bit.ly links hide where they point. " +
                "When on, ShareWhere offers to look — and always asks first, " +
                "naming the host, every single time. Off means it never asks " +
                "and never connects.",
            checked = settings.offerLinkResolution,
            onCheckedChange = { value -> onChange { it.copy(offerLinkResolution = value) } },
        )

        Section("Locations")
        Text(
            "Sharing an exact position is itself a privacy decision. Blurring it " +
                "still gets someone to the right street.",
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.padding(bottom = 8.dp),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PrecisionChip("Exact", Precision.EXACT, settings, onChange)
            PrecisionChip("~100 m", Precision.APPROXIMATE, settings, onChange)
            PrecisionChip("~1 km", Precision.COARSE, settings, onChange)
        }
        SettingSwitch(
            title = "Include place name",
            subtitle = "Off shares only the coordinates. A label like \"Home\" " +
                "often says more than the numbers do.",
            checked = settings.includeLabel,
            onCheckedChange = { value -> onChange { it.copy(includeLabel = value) } },
        )

        Section("Behaviour")
        SettingSwitch(
            title = "Always show the preview",
            subtitle = "Clean Copy normally finishes without a screen. Turn this " +
                "on to see what was removed every time.",
            checked = settings.alwaysPreview,
            onCheckedChange = { value -> onChange { it.copy(alwaysPreview = value) } },
        )
        SettingSwitch(
            title = "Open map links with ShareWhere",
            subtitle = "Adds ShareWhere as an option for geo: and omaps.app links. " +
                "Off by default so installing it does not quietly take over your " +
                "map navigation.",
            checked = handlersEnabled,
            onCheckedChange = { value ->
                LinkHandlers.setEnabled(context, value)
                handlersEnabled = value
            },
        )

        Section("About")
        Text(
            "Version ${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})",
            style = MaterialTheme.typography.bodyMedium,
        )
        Text(
            "One permission: internet, used only when you tap to allow a specific " +
                "link. No analytics, no crash reporting, no clipboard reading.",
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.padding(top = 4.dp),
        )
        TextButton(onClick = onOpenProject, modifier = Modifier.padding(top = 4.dp)) {
            Text(BuildConfig.PROJECT_URL)
        }
        Text(
            "GPL-3.0-or-later. Tracking rules from the ClearURLs project, LGPL-3.0.",
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

@Composable
private fun Section(title: String) {
    HorizontalDivider(modifier = Modifier.padding(top = 20.dp, bottom = 8.dp))
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        modifier = Modifier.padding(bottom = 4.dp),
    )
}

@Composable
private fun SettingSwitch(
    title: String,
    subtitle: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f).padding(end = 12.dp)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(subtitle, style = MaterialTheme.typography.bodySmall)
        }
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

@Composable
private fun PrecisionChip(
    label: String,
    precision: Precision,
    settings: Settings,
    onChange: ((Settings) -> Settings) -> Unit,
) {
    FilterChip(
        selected = settings.precision == precision,
        onClick = { onChange { it.copy(precision = precision) } },
        label = { Text(label) },
    )
}
