# R8 renames the JNA classes UniFFI relies on, which fails at runtime rather
# than at build time. These keeps travel with the module so the app never has to
# remember them.

-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
-dontwarn java.awt.*

# UniFFI's generated bindings are reached reflectively through JNA.
-keep class uniffi.sharewhere.** { *; }

# ShareWhere must never carry a crash reporter or analytics SDK, so there is
# nothing else to keep here. If this file grows, ask why.
