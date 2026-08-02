package app.sharewhere.net

/**
 * The offline flavor has no HTTP client on its classpath at all, so there is
 * nothing here to construct. The UI reads [LinkResolver.available] and explains
 * the situation instead of showing a dead button.
 */
fun createLinkResolver(): LinkResolver = UnavailableLinkResolver
