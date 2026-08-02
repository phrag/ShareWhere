# App-specific R8 rules. The JNA and UniFFI keeps live in
# core-rust/consumer-rules.pro so they travel with the module that needs them.

# Compose and AndroidX ship their own rules; there is deliberately nothing else
# here. If this file grows, ask why -- the app has no reflection of its own.
