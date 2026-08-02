package app.sharewhere.net

import uniffi.sharewhere.Options
import uniffi.sharewhere.Outcome

/**
 * The seam between the UI and the network.
 *
 * `core-net` is only on the `standard` flavor's classpath, so shared UI code
 * cannot reference it directly. Each flavor supplies its own
 * [createLinkResolver]; the `offline` one has no HTTP client to call and says
 * so, rather than offering a button that cannot work.
 */
interface LinkResolver {

    /** Whether this build can make a request at all. */
    val available: Boolean

    /**
     * Follow a short link far enough to find the location behind it.
     *
     * Only ever called after the user has tapped to allow this specific
     * request — the core returns [Outcome.NeedsConsent] until then.
     */
    suspend fun resolve(input: String, options: Options): Outcome
}

/**
 * The offline answer: there is nothing to call.
 *
 * Used directly by the `offline` flavor, and as the fallback anywhere a
 * resolver is needed but networking is not compiled in.
 */
object UnavailableLinkResolver : LinkResolver {
    override val available = false

    override suspend fun resolve(input: String, options: Options): Outcome =
        Outcome.Nothing
}
