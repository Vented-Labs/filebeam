plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "io.filebeam.android"
    compileSdk = 37
    defaultConfig {
        applicationId = "io.filebeam.android"
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0-dev"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk {
            abiFilters += setOf("arm64-v8a", "armeabi-v7a", "x86_64")
            providers.gradleProperty("rustAbis").orNull?.let { abiFilters.retainAll(it.split(',')) }
        }
        // Release automation supplies true only alongside a matching signed-domain assetlinks.json.
        manifestPlaceholders["appLinkHost"] = providers.gradleProperty("appLinkHost").orElse("filebeam.io").get()
        manifestPlaceholders["appLinkAutoVerify"] = providers.gradleProperty("appLinkAutoVerify").orElse("false").get()
    }
    buildTypes {
        debug { applicationIdSuffix = ".debug" }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    buildFeatures { compose = true; buildConfig = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    packaging { resources.excludes += "/META-INF/{AL2.0,LGPL2.1}" }
    lint {
        warningsAsErrors = true
        // Updates are reviewed in the pinned version catalog. A new upstream
        // release must not change the outcome of an otherwise identical build.
        disable += setOf("GradleDependency", "NewerVersionAvailable")
    }
}

dependencies {
    implementation(project(":core-rust"))
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.icons)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.compose)
    implementation(libs.lifecycle.viewmodel)
    implementation(libs.datastore)
    implementation(libs.core)
    implementation(libs.coroutines)
    debugImplementation(libs.compose.tooling)
    debugImplementation(libs.compose.test.manifest)
    testImplementation(libs.junit)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.test)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.test.runner)
}

/**
 * Opt-in release gate. It validates supplied production association evidence;
 * it does not manufacture an assetlinks.json or claim domain verification.
 */
tasks.register("validateVerifiedAppLinks") {
    val enabled = providers.gradleProperty("appLinkAutoVerify").orElse("false")
    val host = providers.gradleProperty("appLinkHost")
    val statement = providers.gradleProperty("appLinkStatementFile")
    val certificate = providers.gradleProperty("appLinkCertificateSha256")
    doLast {
        if (enabled.get().toBooleanStrictOrNull() != true) return@doLast
        val domain = host.orNull?.trim()?.lowercase().orEmpty()
        check(domain.isNotBlank()) { "Verified App Links require -PappLinkHost" }
        val file = statement.orNull?.let(::file)
            ?: error("Verified App Links require -PappLinkStatementFile=<assetlinks.json>")
        check(file.isFile) { "Configured assetlinks.json does not exist" }
        val fingerprint = certificate.orNull?.replace(":", "")?.uppercase().orEmpty()
        check(fingerprint.matches(Regex("[0-9A-F]{64}"))) {
            "Verified App Links require a 64-hex -PappLinkCertificateSha256"
        }
        val association = file.readText().uppercase()
        check(domain.uppercase() in association && fingerprint in association.replace(":", "")) {
            "assetlinks.json does not contain the configured domain and signing certificate"
        }
    }
}
tasks.matching { it.name == "preReleaseBuild" || it.name == "assembleRelease" }
    .configureEach { dependsOn("validateVerifiedAppLinks") }
