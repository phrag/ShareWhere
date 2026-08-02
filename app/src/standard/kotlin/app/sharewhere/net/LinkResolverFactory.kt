package app.sharewhere.net

import uniffi.sharewhere.Options
import uniffi.sharewhere.Outcome

/**
 * The standard flavor delegates to core-net, which drives the Rust core's
 * resolve session and enforces the policy it hands back.
 */
fun createLinkResolver(): LinkResolver = CoordinatorLinkResolver()

private class CoordinatorLinkResolver : LinkResolver {
    private val coordinator = ResolveCoordinator()

    override val available = true

    override suspend fun resolve(input: String, options: Options): Outcome =
        coordinator.resolve(input, options)
}
