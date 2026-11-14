import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// Android library shared by every Selfkit app (sync job, MulticastLock, battery hint,
// signature-protected ContentProvider). Versions of AGP/Kotlin come from the app
// (or from settings.gradle.kts in the standalone CI build).
plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.selfkit.sync"
    compileSdk = 36
    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}

kotlin {
    compilerOptions { jvmTarget = JvmTarget.JVM_1_8 }
}

dependencies {
    implementation("androidx.work:work-runtime-ktx:2.10.1")
    implementation("androidx.core:core-ktx:1.16.0")
}
