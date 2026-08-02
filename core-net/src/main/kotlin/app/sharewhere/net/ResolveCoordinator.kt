package app.sharewhere.net

import java.net.InetAddress
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.CookieJar
import okhttp3.OkHttpClient
import okhttp3.Request
import uniffi.sharewhere.FetchRequest
import uniffi.sharewhere.FetchResponse
import uniffi.sharewhere.HttpMethod
import uniffi.sharewhere.Options
import uniffi.sharewhere.Outcome
import uniffi.sharewhere.ResolveSession
import uniffi.sharewhere.Step

/**
 * Drives a [ResolveSession] to completion, turning its request/response loop
 * into a single suspend call.
 *
 * This module moves bytes and nothing else. It does not decide what to fetch,
 * does not follow redirects, and does not parse anything -- all of that is in
 * the Rust core, where it is testable and where the policy lives in one place.
 *
 * The policy on each [FetchRequest] is re-checked here rather than trusted.
 * That duplication is deliberate: the redirect target is chosen by whoever
 * controls the link, so both layers verify it.
 */
class ResolveCoordinator(
    private val client: OkHttpClient = defaultClient(),
) {

    suspend fun resolve(input: String, options: Options): Outcome =
        withContext(Dispatchers.IO) {
            val session = ResolveSession(input, options)
            while (true) {
                when (val step = session.advance()) {
                    is Step.Done -> return@withContext step.outcome
                    is Step.Fetch -> session.supply(perform(step.request))
                }
            }
            @Suppress("UNREACHABLE_CODE")
            Outcome.Nothing
        }

    private fun perform(request: FetchRequest): FetchResponse {
        require(isAllowed(request)) { "request violates the policy the core set" }

        val call = Request.Builder()
            .url(request.url)
            .header("User-Agent", request.userAgent)
            // Never negotiate anything that could identify the device.
            .header("Accept", "text/html,*/*;q=0.5")
            .apply {
                when (request.method) {
                    HttpMethod.HEAD -> head()
                    HttpMethod.GET -> get()
                }
            }
            .build()

        val timed = client.newBuilder()
            .callTimeout(request.timeoutMs.toLong(), TimeUnit.MILLISECONDS)
            .followRedirects(request.followRedirects)
            .followSslRedirects(request.followRedirects)
            .build()

        timed.newCall(call).execute().use { response ->
            val body = if (request.method == HttpMethod.GET) {
                // Bounded read: a hostile page must not be able to make us
                // buffer megabytes.
                response.body?.source()?.let { source ->
                    source.request(request.maxBodyBytes.toLong())
                    source.buffer.snapshot(
                        minOf(source.buffer.size, request.maxBodyBytes.toLong()).toInt(),
                    ).utf8()
                }
            } else {
                null
            }

            return FetchResponse(
                status = response.code.toUShort(),
                finalUrl = response.request.url.toString(),
                locationHeader = response.header("Location"),
                body = body,
            )
        }
    }

    /**
     * Re-check what the core already asserted.
     *
     * https only, host on the allow-list, and never a private or loopback
     * address -- the last of which stops a hostile redirect turning this into
     * an SSRF primitive against whatever the phone can reach.
     */
    private fun isAllowed(request: FetchRequest): Boolean {
        val url = runCatching { okhttp3.HttpUrl.get(request.url) }.getOrNull() ?: return false
        if (url.scheme() != "https") return false
        if (request.allowedHosts.none { it.equals(url.host(), ignoreCase = true) }) return false

        return runCatching {
            InetAddress.getAllByName(url.host()).none {
                it.isLoopbackAddress || it.isSiteLocalAddress || it.isLinkLocalAddress ||
                    it.isAnyLocalAddress || it.isMulticastAddress
            }
        }.getOrDefault(false)
    }

    companion object {
        private fun defaultClient() = OkHttpClient.Builder()
            // A cookie jar would turn link-cleaning into tracked browsing.
            .cookieJar(CookieJar.NO_COOKIES)
            // The core walks redirects itself so it can count hops and re-check
            // the host at each one.
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(false)
            .connectTimeout(8, TimeUnit.SECONDS)
            .readTimeout(8, TimeUnit.SECONDS)
            .build()
    }
}
