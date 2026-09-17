package io.filebeam.android.platform.background

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ExecutionGateTest {
    @Test fun replacementWaitsForStoppedWorkerToReleaseResources() = runBlocking {
        val gate = ExecutionGate()
        val firstEntered = CompletableDeferred<Unit>()
        val releaseFirst = CompletableDeferred<Unit>()
        val secondEntered = CompletableDeferred<Unit>()
        val first = async { gate.run { firstEntered.complete(Unit); releaseFirst.await() } }
        firstEntered.await()
        val second = async { gate.run { secondEntered.complete(Unit) } }
        assertFalse(secondEntered.isCompleted)
        releaseFirst.complete(Unit)
        awaitAll(first, second)
        assertTrue(secondEntered.isCompleted)
    }
}
