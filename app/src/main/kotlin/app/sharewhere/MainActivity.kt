package app.sharewhere

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * The home screen. Mostly a place to explain the app and reach settings: the
 * real entry point is the share sheet.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            Surface(modifier = Modifier.fillMaxSize()) {
                Column(modifier = Modifier.padding(24.dp)) {
                    Text("ShareWhere", style = MaterialTheme.typography.headlineMedium)
                    Text(
                        "Share a link to ShareWhere and it comes back without the " +
                            "tracking. Share a location and you get every map format at once.",
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(top = 12.dp),
                    )
                    Text(
                        if (BuildConfig.NETWORK_AVAILABLE) {
                            "This build can follow short links, but only when you tap to allow it."
                        } else {
                            "This build has no internet permission at all."
                        },
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(top = 16.dp),
                    )
                }
            }
        }
    }
}
