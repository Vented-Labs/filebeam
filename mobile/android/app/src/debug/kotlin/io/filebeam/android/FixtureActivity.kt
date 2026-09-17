package io.filebeam.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import io.filebeam.android.ui.FilebeamScreen
import io.filebeam.android.ui.FilebeamViewModel

/**
 * Debug-only evidence host. This deliberately renders the production root with
 * its real ViewModel; it does not provide a parallel UI or transfer simulator.
 */
class FixtureActivity : ComponentActivity() {
    private val model: FilebeamViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) intent.getStringExtra(EXTRA_ROUTE)?.let(model::navigateLegacy)
        setContent {
            FilebeamScreen(
                model = model,
                start = { action -> action() },
                // Pick/export actions are intentionally unavailable in a visual fixture.
                // Production MainActivity owns Android activity-result integration.
                pickFiles = {},
                pickTree = {},
                saveFile = {},
                exportAccountKey = {},
                importAccountKey = {},
            )
        }
    }

    companion object { const val EXTRA_ROUTE = "io.filebeam.android.fixture.ROUTE" }
}
