package app.sharewhere

import android.app.Application
import uniffi.sharewhere.warmUp

class ShareWhereApp : Application() {
    override fun onCreate() {
        super.onCreate()
        // Compile the always-on rules now, so the first share of a session is
        // already warm rather than paying the 2-5 ms cold cost in front of the
        // user. Everything else compiles lazily, per host.
        warmUp()
    }
}
