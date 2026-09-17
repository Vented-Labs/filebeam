# JNA finds these interfaces/structures reflectively, including generated UniFFI bindings.
-keep class com.sun.jna.** { *; }
-keep class io.filebeam.rust.** { *; }
# JNA's optional desktop window helpers reference AWT, which Android does not
# provide. UniFFI uses only its C-library interfaces and never calls these helpers.
-dontwarn java.awt.**
