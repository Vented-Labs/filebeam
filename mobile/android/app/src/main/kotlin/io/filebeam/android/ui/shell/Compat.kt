package io.filebeam.android.ui.shell

import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.StateFlow

/** Collects a state flow only while the screen is at least started. */
@Composable
fun <T> StateFlow<T>.collectAsStateCompat(): State<T> = collectAsStateWithLifecycle()
