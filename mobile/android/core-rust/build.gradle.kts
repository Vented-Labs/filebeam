plugins { alias(libs.plugins.android.library) }

val repository = rootDir.resolve("../..").canonicalFile
val generated = layout.buildDirectory.dir("generated/rust")
val abis = providers.gradleProperty("rustAbis").orElse("arm64-v8a,armeabi-v7a,x86_64")
val profile = providers.gradleProperty("rustProfile").orElse("release")

val buildRust = tasks.register<Exec>("buildRust") {
    workingDir(repository)
    inputs.files(fileTree(repository) {
        include("client-ffi/Cargo.*", "client-ffi/rust-toolchain.toml", "client-ffi/uniffi.toml", "client-ffi/src/**")
        include("client-core/Cargo.*", "client-core/src/**")
        include("transfer-native/Cargo.*", "transfer-native/src/**")
        include("transfer/Cargo.*", "transfer/src/**", "encryption/Cargo.*", "encryption/src/**")
        include("scripts/android/build-rust.sh")
    })
    inputs.property("abis", abis)
    inputs.property("profile", profile)
    outputs.dir(generated)
    commandLine("bash", repository.resolve("scripts/android/build-rust.sh"), generated.get().asFile, abis.get(), profile.get())
}

android {
    namespace = "io.filebeam.core"
    compileSdk = 37
    ndkVersion = "28.2.13676358"
    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
androidComponents.onVariants { variant ->
    variant.sources.java?.addStaticSourceDirectory(generated.get().dir("kotlin").asFile.path)
    variant.sources.jniLibs?.addStaticSourceDirectory(generated.get().dir("jniLibs").asFile.path)
}
tasks.named("preBuild") { dependsOn(buildRust) }

dependencies {
    api("net.java.dev.jna:jna:${libs.versions.jna.get()}@aar")
    api(libs.coroutines)
}
