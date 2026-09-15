import java.io.File
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

val testSigningPropertiesFile = providers.gradleProperty("testSigningPropertiesFile")
    .map { path -> file(path) }
    .orElse(rootDir.resolve("../../.filebeam/android-test-signing.properties"))
val testSigningProperties = Properties()
val hasTestSigning = testSigningPropertiesFile.get().isFile

if (hasTestSigning) {
    testSigningPropertiesFile.get().inputStream().use(testSigningProperties::load)
    val required = setOf("storeFile", "storePassword", "keyAlias", "keyPassword")
    check(required.all(testSigningProperties::containsKey)) {
        "Test signing properties must define: ${required.joinToString(", ")}"
    }
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
    signingConfigs {
        create("testRelease") {
            if (hasTestSigning) {
                val configuredStore = File(testSigningProperties.getProperty("storeFile"))
                storeFile = if (configuredStore.isAbsolute) configuredStore else testSigningPropertiesFile.get().parentFile.resolve(configuredStore)
                storePassword = testSigningProperties.getProperty("storePassword")
                keyAlias = testSigningProperties.getProperty("keyAlias")
                keyPassword = testSigningProperties.getProperty("keyPassword")
            }
        }
    }
    buildTypes {
        debug { applicationIdSuffix = ".debug" }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (hasTestSigning) signingConfig = signingConfigs.getByName("testRelease")
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

// This consumes an existing R8 APK so post-build signing checks do not trigger a
// native rebuild. The ignored key is test-only and never represents production.
tasks.register("signedR8Smoke") {
    group = "verification"
    description = "Checks, test-signs, and verifies an existing unsigned R8 release APK."
    onlyIf { hasTestSigning }
    doLast {
        val unsigned = layout.buildDirectory.file("outputs/apk/release/app-release-unsigned.apk").get().asFile
        val signed = layout.buildDirectory.file("outputs/apk/release/app-release-test-signed.apk").get().asFile
        val configuredStore = File(testSigningProperties.getProperty("storeFile"))
        val keystore = if (configuredStore.isAbsolute) configuredStore else testSigningPropertiesFile.get().parentFile.resolve(configuredStore)
        check(unsigned.isFile) { "Build the unsigned R8 APK before running signedR8Smoke: ${unsigned.path}" }
        check(keystore.isFile) { "Configured test keystore does not exist" }
        fun run(vararg command: String) {
            val renderedCommand = command.joinToString(" ")
            check(ProcessBuilder(*command).inheritIO().start().waitFor() == 0) {
                "Command failed: $renderedCommand"
            }
        }
        run("python3", rootDir.resolve("../../scripts/android/check-native.py").path, unsigned.path)
        signed.delete()
        run(
            "apksigner", "sign", "--ks", keystore.path,
            "--ks-key-alias", testSigningProperties.getProperty("keyAlias"),
            "--ks-pass", "pass:${testSigningProperties.getProperty("storePassword")}",
            "--key-pass", "pass:${testSigningProperties.getProperty("keyPassword")}",
            "--out", signed.path, unsigned.path,
        )
        run("apksigner", "verify", "--verbose", signed.path)
    }
}
