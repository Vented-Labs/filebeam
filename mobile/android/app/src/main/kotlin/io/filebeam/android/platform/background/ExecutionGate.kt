package io.filebeam.android.platform.background

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** A replacement worker waits until the stopped worker has released native resources. */
internal class ExecutionGate {
    private val mutex = Mutex()
    suspend fun <T> run(block: suspend () -> T): T = mutex.withLock { block() }
}
