package app.sharewhere.settings

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager

/**
 * Turning the map-link handlers on and off.
 *
 * The `geo:`, `om:` and `omaps.app` intent filters live on activity-aliases
 * that ship **disabled**. An app that grabs your map links the moment it is
 * installed has taken a decision that is yours to make, so ShareWhere waits to
 * be asked.
 */
object LinkHandlers {

    private const val GEO = "app.sharewhere.share.GeoLinkHandler"
    private const val MAP_WEB = "app.sharewhere.share.MapWebLinkHandler"

    fun enabled(context: Context): Boolean = isEnabled(context, GEO)

    fun setEnabled(context: Context, enabled: Boolean) {
        setComponent(context, GEO, enabled)
        setComponent(context, MAP_WEB, enabled)
    }

    private fun isEnabled(context: Context, className: String): Boolean =
        context.packageManager.getComponentEnabledSetting(
            ComponentName(context.packageName, className),
        ) == PackageManager.COMPONENT_ENABLED_STATE_ENABLED

    private fun setComponent(context: Context, className: String, enabled: Boolean) {
        context.packageManager.setComponentEnabledSetting(
            ComponentName(context.packageName, className),
            if (enabled) {
                PackageManager.COMPONENT_ENABLED_STATE_ENABLED
            } else {
                PackageManager.COMPONENT_ENABLED_STATE_DISABLED
            },
            // Without DONT_KILL_APP this call terminates the process, which
            // looks exactly like a crash to whoever just tapped the switch.
            PackageManager.DONT_KILL_APP,
        )
    }
}
