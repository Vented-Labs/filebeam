package io.filebeam.android.ui

import org.junit.Assert.assertTrue
import org.junit.Test

class ThemeFallbackTest {
    @Test fun allFallbackSemanticContentPairsMeetNormalTextContrast() {
        val pairs = listOf(FilebeamLightFallback, FilebeamDarkFallback).flatMap { scheme -> listOf(
            scheme.onPrimary to scheme.primary,
            scheme.onPrimaryContainer to scheme.primaryContainer,
            scheme.onSecondary to scheme.secondary,
            scheme.onSecondaryContainer to scheme.secondaryContainer,
            scheme.onTertiary to scheme.tertiary,
            scheme.onTertiaryContainer to scheme.tertiaryContainer,
            scheme.onError to scheme.error,
            scheme.onErrorContainer to scheme.errorContainer,
            scheme.onBackground to scheme.background,
            scheme.onSurface to scheme.surface,
            scheme.onSurfaceVariant to scheme.surfaceVariant,
            scheme.inverseOnSurface to scheme.inverseSurface,
            scheme.inversePrimary to scheme.inverseSurface,
        ) }

        pairs.forEach { (foreground, background) ->
            assertTrue("Expected 4.5:1 contrast, was ${contrastRatio(foreground, background)}", contrastRatio(foreground, background) >= 4.5f)
        }
    }
}
