// Standalone build (CI compile check: .github/workflows/sync-e2e.yml). Apps include
// this folder as a module instead; see README.md.
pluginManagement {
    repositories { google(); mavenCentral(); gradlePluginPortal() }
    // Versions only for the standalone build; inside an app the app's own AGP/Kotlin are used.
    plugins {
        id("com.android.library") version "9.3.1"
        id("org.jetbrains.kotlin.android") version "2.2.10"
    }
}
dependencyResolutionManagement {
    repositories { google(); mavenCentral() }
}
rootProject.name = "selfkit-sync"
