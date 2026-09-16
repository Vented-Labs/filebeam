package io.filebeam.android.ui

import org.junit.Assert.assertTrue
import org.junit.Test

class ThemeFallbackTest {
    @Test fun fallbackSemanticTextPairsMeetNormalTextContrast() {
        val pairs = listOf(
            FilebeamLightFallback.onSurface to FilebeamLightFallback.surface,
            FilebeamLightFallback.onPrimary to FilebeamLightFallback.primary,
            FilebeamLightFallback.onError to FilebeamLightFallback.error,
            FilebeamDarkFallback.onSurface to FilebeamDarkFallback.surface,
            FilebeamDarkFallback.onPrimary to FilebeamDarkFallback.primary,
            FilebeamDarkFallback.onError to FilebeamDarkFallback.error,
        )

        pairs.forEach { (foreground, background) ->
            assertTrue("Expected 4.5:1 contrast, was ${contrastRatio(foreground, background)}", contrastRatio(foreground, background) >= 4.5f)
        }
    }
}
