# Drop Receiver ProGuard rules

# Keep data classes for Gson serialization
-keep class dev.drop.receiver.** { *; }

# Gson
-keepattributes Signature
-keepattributes *Annotation*
-dontwarn sun.misc.**
-keep class com.google.gson.** { *; }

# NanoHTTPD
-keep class fi.iki.elonen.** { *; }
