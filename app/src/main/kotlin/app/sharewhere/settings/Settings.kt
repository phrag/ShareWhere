package app.sharewhere.settings

import android.content.Context
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import uniffi.sharewhere.Options
import uniffi.sharewhere.Precision
import uniffi.sharewhere.defaultOptions

/**
 * Everything the user can change, and nothing else.
 *
 * Stored in app-private storage and excluded from cloud backup by
 * `data_extraction_rules.xml` — these are preferences, not something that
 * should follow you onto another device.
 */
data class Settings(
    /** Strip affiliate parameters as well as pure tracking ones. */
    val removeReferral: Boolean = true,

    /** Unwrap links that only wrap a destination, like `google.com/url?q=`. */
    val followRedirects: Boolean = true,

    /** How precisely to share a location. */
    val precision: Precision = Precision.EXACT,

    /** Include the place name. "Home" often says more than the coordinates. */
    val includeLabel: Boolean = true,

    /**
     * Show the preview sheet even for Clean Copy.
     *
     * Off by default: Clean Copy exists to be instant. The brief popup naming
     * what was removed covers the common case.
     */
    val alwaysPreview: Boolean = false,

    /** Offer to resolve short links at all. Off means never touch the network. */
    val offerLinkResolution: Boolean = true,
) {
    /** The core's options, with the user's choices applied. */
    fun toOptions(): Options = defaultOptions().copy(
        removeReferral = removeReferral,
        followRedirectParams = followRedirects,
        precision = precision,
        includeLabel = includeLabel,
        // Always false here. Network access is granted per resolve, by tapping
        // through the consent dialog — never from stored settings.
        allowNetwork = false,
    )
}

private val Context.dataStore by preferencesDataStore(name = "settings")

class SettingsRepository(private val context: Context) {

    val settings: Flow<Settings> = context.dataStore.data.map(::read)

    suspend fun update(transform: (Settings) -> Settings) {
        context.dataStore.edit { prefs ->
            val next = transform(read(prefs))
            prefs[REMOVE_REFERRAL] = next.removeReferral
            prefs[FOLLOW_REDIRECTS] = next.followRedirects
            prefs[PRECISION] = next.precision.name
            prefs[INCLUDE_LABEL] = next.includeLabel
            prefs[ALWAYS_PREVIEW] = next.alwaysPreview
            prefs[OFFER_RESOLUTION] = next.offerLinkResolution
        }
    }

    private fun read(prefs: Preferences) = Settings(
        removeReferral = prefs[REMOVE_REFERRAL] ?: true,
        followRedirects = prefs[FOLLOW_REDIRECTS] ?: true,
        // An unknown stored value falls back to the default rather than
        // throwing: a renamed enum variant should not brick the app.
        precision = prefs[PRECISION]
            ?.let { name -> Precision.entries.firstOrNull { it.name == name } }
            ?: Precision.EXACT,
        includeLabel = prefs[INCLUDE_LABEL] ?: true,
        alwaysPreview = prefs[ALWAYS_PREVIEW] ?: false,
        offerLinkResolution = prefs[OFFER_RESOLUTION] ?: true,
    )

    private companion object {
        val REMOVE_REFERRAL = booleanPreferencesKey("remove_referral")
        val FOLLOW_REDIRECTS = booleanPreferencesKey("follow_redirects")
        val PRECISION = stringPreferencesKey("precision")
        val INCLUDE_LABEL = booleanPreferencesKey("include_label")
        val ALWAYS_PREVIEW = booleanPreferencesKey("always_preview")
        val OFFER_RESOLUTION = booleanPreferencesKey("offer_link_resolution")
    }
}
