package io.filebeam.android.platform.devices

import android.content.Context
import android.os.Build
import android.util.Base64
import io.filebeam.android.BuildConfig
import io.filebeam.android.platform.security.PendingTransferStore
import io.filebeam.android.platform.services.AccountSessionRegistry
import io.filebeam.rust.DeviceCredentialStoreCallback
import io.filebeam.rust.DeviceApprovalPayload
import io.filebeam.rust.deviceKeyFingerprint
import io.filebeam.rust.DeviceKeySignerCallback
import io.filebeam.rust.NativeServices
import io.filebeam.rust.NativeRuntime
import io.filebeam.rust.NativeDeviceSigner
import io.filebeam.rust.DevicePeerProofRequest
import io.filebeam.rust.DeviceReceiptRequest
import io.filebeam.rust.DeviceSendRequest
import io.filebeam.android.platform.DeviceJobLifecycle
import io.filebeam.rust.RegisteredDevice
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.SecureRandom
import java.security.Signature
import java.security.spec.PKCS8EncodedKeySpec

/** Safe-to-render registry state. Immutable IDs, not names, select devices. */
data class DeviceDirectory(
    val instance: String,
    val generation: ULong,
    val revision: ULong,
    val currentDeviceId: String?,
    val devices: List<DeviceEntry>,
    val stale: Boolean,
    val detail: String? = null,
)

data class DeviceEntry(
    val id: String,
    val name: String,
    val platform: String,
    val trust: DeviceTrust,
    val availability: DeviceAvailability,
    val detail: String? = null,
    val canReceive: Boolean,
)

enum class DeviceTrust { PENDING, VERIFIED, REVOKED }
enum class DeviceAvailability { READY, BUSY, NEEDS_OPEN, LOCKED, OFFLINE, UNKNOWN }

/**
 * Runtime-backed implementations must use the active account-instance context.
 * This boundary deliberately does not expose offers, jobs, paths, manifests,
 * credentials, or peer-private metadata.
 */
interface DeviceRegistry {
    suspend fun directory(instance: String): DeviceDirectory
    suspend fun enroll(instance: String, name: String, platform: String): DeviceDirectory
    suspend fun rename(instance: String, deviceId: String, name: String): DeviceDirectory
    suspend fun revoke(instance: String, deviceId: String, currentPassword: String): DeviceDirectory
    suspend fun pendingApprovalPayload(instance: String): String?
    suspend fun approvePayload(instance: String, payload: String, confirmedFingerprint: String): DeviceDirectory
    suspend fun confirmApprover(instance: String, confirmedFingerprint: String): DeviceDirectory
    suspend fun send(instance: String, sourceName: String, providerIdentity: String, providerLength: Long, targetDeviceId: String): ULong
}

class DeviceRegistryUnavailable : IllegalStateException("Device transfers are unavailable until this Filebeam runtime supports registered devices.")

/** Installed until UniFFI exposes the daemon/application-owned registry surface. */
object UnavailableDeviceRegistry : DeviceRegistry {
    override suspend fun directory(instance: String): DeviceDirectory = throw DeviceRegistryUnavailable()
    override suspend fun enroll(instance: String, name: String, platform: String): DeviceDirectory = throw DeviceRegistryUnavailable()
    override suspend fun rename(instance: String, deviceId: String, name: String): DeviceDirectory = throw DeviceRegistryUnavailable()
    override suspend fun revoke(instance: String, deviceId: String, currentPassword: String): DeviceDirectory = throw DeviceRegistryUnavailable()
    override suspend fun pendingApprovalPayload(instance: String): String? = throw DeviceRegistryUnavailable()
    override suspend fun approvePayload(instance: String, payload: String, confirmedFingerprint: String): DeviceDirectory = throw DeviceRegistryUnavailable()
    override suspend fun confirmApprover(instance: String, confirmedFingerprint: String): DeviceDirectory = throw DeviceRegistryUnavailable()
    override suspend fun send(instance: String, sourceName: String, providerIdentity: String, providerLength: Long, targetDeviceId: String): ULong = throw DeviceRegistryUnavailable()
}

/**
 * Account registry adapter only. It deliberately does not claim device-send
 * support: peer jobs remain unavailable until the shared runtime owns them.
 */
class NativeDeviceRegistry(
    private val context: Context,
    private val sessions: AccountSessionRegistry,
    private val runtime: NativeRuntime,
    private val jobs: DeviceJobLifecycle,
) : DeviceRegistry {
    private fun pending(origin: String) = PendingTransferStore(context, "device-approval-${digest(origin)}")
    override suspend fun directory(instance: String): DeviceDirectory {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        return directory(origin, service(origin), identity(origin).id)
    }

    override suspend fun enroll(instance: String, name: String, platform: String): DeviceDirectory {
        require(Build.VERSION.SDK_INT >= 31) { "Registered-device signing requires Android 12 or newer." }
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val identity = identity(origin)
        val service = service(origin)
        val challenge = service.accountDeviceChallenge(identity.id, name, platform, identity.publicKey, 1u, emptyList())
        service.accountDeviceEnroll(challenge)
        identity.rememberDeployment(challenge.approvalContext.deploymentId)
        pending(origin).save(org.json.JSONObject().put("context", approvalJson(challenge.approvalContext)).put("challenge", challenge.challenge).put("expires_at", challenge.expiresAt).put("fingerprint", fingerprint(challenge.approvalContext.candidatePublicKey)))
        return directory(origin, service, identity.id)
    }

    override suspend fun rename(instance: String, deviceId: String, name: String): DeviceDirectory {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val service = service(origin)
        service.accountDeviceRename(deviceId, name)
        return directory(origin, service, identity(origin).id)
    }

    override suspend fun revoke(instance: String, deviceId: String, currentPassword: String): DeviceDirectory {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val service = service(origin)
        service.accountDeviceRevoke(deviceId, currentPassword)
        return directory(origin, service, identity(origin).id)
    }

    override suspend fun pendingApprovalPayload(instance: String): String? {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        return pending(origin).load()?.optString("context")?.takeIf(String::isNotBlank)
    }

    override suspend fun approvePayload(instance: String, payload: String, confirmedFingerprint: String): DeviceDirectory {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        require(payload.length <= 4096) { "Pairing payload is too large." }
        val identity = identity(origin)
        val json = org.json.JSONObject(payload)
        val approval = DeviceApprovalPayload(
            json.requiredString("instance_origin"), json.requiredString("deployment_id"), json.requiredLong("account_id").toULong(),
            json.requiredLong("generation").toULong(), json.requiredString("candidate_device_id"), json.requiredLong("candidate_key_version").toUInt(),
            json.requiredString("candidate_public_key"), json.requiredString("challenge_sha256"), json.requiredLong("expires_at_unix").toULong(), identity.id, 1u,
        )
        require(approval.instanceOrigin == origin) { "Pairing payload belongs to another instance." }
        require(fingerprint(approval.candidatePublicKey) == confirmedFingerprint) { "Confirm the candidate key fingerprint shown by the other device." }
        val service = service(origin)
        service.deviceApproveEnrollment(identity.id, approval)
        return directory(origin, service, identity.id)
    }

    override suspend fun confirmApprover(instance: String, confirmedFingerprint: String): DeviceDirectory {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val pending = pending(origin).load() ?: error("No pending approved-device evidence is available.")
        val context = pending.optString("context")
        val evidence = pending.optJSONObject("approval_evidence") ?: error("This device has not redeemed its approved credential yet.")
        val identity = identity(origin)
        val service = service(origin)
        service.deviceConfirmApprover(identity.id, approvalFromJson(org.json.JSONObject(context), identity.id, identity.publicKey), io.filebeam.rust.DeviceApprovalEvidence(
            evidence.requiredLong("version").toUByte(), evidence.requiredString("approver_device_id"), evidence.requiredLong("approver_key_version").toUInt(), evidence.requiredString("approver_public_key"), evidence.requiredString("approval_signature"),
        ), confirmedFingerprint)
        pending(origin).clear()
        return directory(origin, service, identity.id)
    }

    override suspend fun send(instance: String, sourceName: String, providerIdentity: String, providerLength: Long, targetDeviceId: String): ULong {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        require(providerLength >= 0) { "Selected document size is unavailable." }
        val identity = identity(origin)
        val deployment = identity.deploymentId ?: error("Complete device pairing before sending to a device.")
        val service = service(origin)
        jobs.configure(service, deployment, identity.id, Base64.decode(identity.publicKey, Base64.NO_WRAP), PeerSigner(identity))
        return jobs.startSender(DeviceSendRequest(sourceName, null, providerIdentity, providerLength.toULong(), providerIdentity, targetDeviceId, "incoming", (System.currentTimeMillis() / 1000 + 120).toULong()))
    }

    private fun approvalJson(value: DeviceApprovalPayload): String = org.json.JSONObject()
        .put("version", 2).put("instance_origin", value.instanceOrigin).put("deployment_id", value.deploymentId)
        .put("account_id", value.accountId.toString()).put("generation", value.generation.toString())
        .put("candidate_device_id", value.candidateDeviceId).put("candidate_key_version", value.candidateKeyVersion.toString())
        .put("candidate_public_key", value.candidatePublicKey).put("challenge_sha256", value.challengeSha256)
        .put("expires_at_unix", value.expiresAtUnix.toString())
        .toString()

    private fun approvalFromJson(json: org.json.JSONObject, candidateId: String, candidateKey: String) = DeviceApprovalPayload(
        json.requiredString("instance_origin"), json.requiredString("deployment_id"), json.requiredLong("account_id").toULong(),
        json.requiredLong("generation").toULong(), candidateId, json.requiredLong("candidate_key_version").toUInt(), candidateKey,
        json.requiredString("challenge_sha256"), json.requiredLong("expires_at_unix").toULong(), "", 0u,
    )

    private fun fingerprint(key: String) = deviceKeyFingerprint(key)
    private fun org.json.JSONObject.requiredString(name: String) = optString(name).takeIf(String::isNotBlank) ?: error("Pairing payload is missing $name.")
    private fun org.json.JSONObject.requiredLong(name: String) = optString(name).toLongOrNull() ?: error("Pairing payload has an invalid $name.")

    private fun service(origin: String): NativeServices {
        val cookie = sessions.cookie(origin) ?: error("Sign in before managing registered devices")
        val identity = identity(origin)
        return NativeServices.newWithRuntimeDeviceCallbacks(
            origin,
            BuildConfig.DEBUG,
            cookie,
            runtime,
            Signer(identity),
            Credentials(context, origin),
        )
    }

    private fun directory(origin: String, service: NativeServices, current: String): DeviceDirectory {
        var registry = service.accountDevices()
        val pending = pending(origin).load()
        if (pending != null && !pending.optBoolean("credential_redeemed") && registry.devices.any { it.id == current && it.status == "active" }) {
            val context = pending.optString("context")
            val challenge = pending.optString("challenge")
            if (context.isNotBlank() && challenge.isNotBlank()) {
                val candidate = registry.devices.first { it.id == current }
                val approval = approvalFromJson(org.json.JSONObject(context), current, candidate.publicKey)
                val redemption = service.accountDeviceCompleteEnrollment(io.filebeam.rust.DeviceEnrollmentChallenge(candidate, challenge, pending.optString("expires_at").takeIf(String::isNotBlank), approval))
                val evidence = redemption.approvalEvidence ?: error("Approved enrollment did not return approval evidence.")
                pending.put("credential_redeemed", true).put("approval_evidence", org.json.JSONObject()
                    .put("version", evidence.version.toString()).put("approver_device_id", evidence.approverDeviceId)
                    .put("approver_key_version", evidence.approverKeyVersion.toString()).put("approver_public_key", evidence.approverPublicKey)
                    .put("approval_signature", evidence.approvalSignature))
                pending(origin).save(pending)
                registry = service.accountDevices()
            }
        }
        val identity = identity(origin)
        // A local rendezvous poll says nothing about a remote device's OS
        // lifetime or presence. Do not turn it into a readiness assertion.
        identity.deploymentId?.let { deployment ->
            runCatching {
                jobs.configure(service, deployment, identity.id, Base64.decode(identity.publicKey, Base64.NO_WRAP), PeerSigner(identity))
            }
        }
        val trusted = runCatching { jobs.registeredTrustSnapshot().filter { it.pinned }.map { it.deviceId }.toSet() }.getOrDefault(emptySet())
        return DeviceDirectory(
            origin,
            registry.generation,
            registry.revision,
            current,
            registry.devices.map { entry(it, it.id in trusted) },
            stale = false,
        )
    }

    private fun entry(device: RegisteredDevice, pinned: Boolean) = DeviceEntry(
        device.id,
        device.displayName,
        device.platform,
        when {
            device.status == "revoked" -> DeviceTrust.REVOKED
            device.status == "pending" -> DeviceTrust.PENDING
            pinned -> DeviceTrust.VERIFIED
            else -> DeviceTrust.PENDING
        },
        when {
            device.presenceExpiresAt?.let(::presenceIsLive) == true -> DeviceAvailability.READY
            device.status == "active" -> DeviceAvailability.NEEDS_OPEN
            else -> DeviceAvailability.OFFLINE
        },
        // A trusted sleeping device remains a valid queued-send target. Readiness
        // answers whether it is presently reachable, not whether it is trusted.
        detail = if (device.status == "active" && !pinned) "Pairing needed" else null,
        canReceive = pinned && device.status == "active",
    )

    private fun presenceIsLive(value: String): Boolean = runCatching {
        java.time.Instant.parse(value).isAfter(java.time.Instant.now())
    }.getOrDefault(false)

    private fun identity(origin: String) = DeviceIdentity(PendingTransferStore(context, "device-identity-${digest(origin)}"))
    private fun digest(value: String) = java.security.MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }

    private class Credentials(context: Context, origin: String) : DeviceCredentialStoreCallback {
        private val store = PendingTransferStore(context, "device-credential-${java.security.MessageDigest.getInstance("SHA-256").digest(origin.toByteArray()).joinToString("") { "%02x".format(it) }}")
        override fun store(instance: String, deviceId: String, credential: String) = store.save(org.json.JSONObject().put("id", deviceId).put("credential", credential))
        override fun load(instance: String, deviceId: String): String = store.load()?.takeIf { it.optString("id") == deviceId }?.optString("credential")?.takeIf(String::isNotBlank) ?: error("This device credential is unavailable. Register again.")
        override fun remove(instance: String, deviceId: String) { if (store.load()?.optString("id") == deviceId) store.clear() }
    }

    private class DeviceIdentity(private val store: PendingTransferStore) {
        val id: String
        val publicKey: String
        private val privateKey: java.security.PrivateKey
        val deploymentId: String? get() = store.load()?.optString("deployment").takeIf { !it.isNullOrBlank() }
        init {
            val saved = store.load()
            if (saved != null) {
                id = saved.getString("id")
                publicKey = saved.getString("public")
                privateKey = KeyFactory.getInstance("Ed25519").generatePrivate(PKCS8EncodedKeySpec(Base64.decode(saved.getString("private"), Base64.NO_WRAP)))
            } else {
                val pair = KeyPairGenerator.getInstance("Ed25519").generateKeyPair()
                id = ulid()
                publicKey = Base64.encodeToString(pair.public.encoded.takeLast(32).toByteArray(), Base64.NO_WRAP)
                privateKey = pair.private
                store.save(org.json.JSONObject().put("id", id).put("public", publicKey).put("private", Base64.encodeToString(pair.private.encoded, Base64.NO_WRAP)))
            }
        }
        fun sign(bytes: ByteArray): ByteArray = Signature.getInstance("Ed25519").run { initSign(privateKey); update(bytes); sign() }
        fun rememberDeployment(value: String) {
            val record = store.load() ?: error("Device identity is unavailable")
            record.put("deployment", value)
            store.save(record)
        }
        private fun ulid(): String {
            val alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
            var time = System.currentTimeMillis(); val chars = CharArray(26)
            for (index in 9 downTo 0) { chars[index] = alphabet[(time and 31).toInt()]; time = time ushr 5 }
            val random = SecureRandom(); for (index in 10..25) chars[index] = alphabet[random.nextInt(alphabet.length)]
            return String(chars)
        }
    }

    private class Signer(private val identity: DeviceIdentity) : DeviceKeySignerCallback {
        override fun signEnrollment(deviceId: String, generation: ULong, challenge: String): ByteArray = identity.sign("filebeam-device-enrollment-v1\u0000$deviceId\u0000$generation\u0000".toByteArray() + Base64.decode(challenge, Base64.NO_WRAP))
        override fun signApproval(approval: io.filebeam.rust.DeviceApprovalPayload): ByteArray {
            require(approval.instanceOrigin == approval.instanceOrigin.trimEnd('/')) { "Invalid approval origin" }
            val challengeDigest = Base64.decode(approval.challengeSha256, Base64.NO_WRAP)
            val key = Base64.decode(approval.candidatePublicKey, Base64.NO_WRAP)
            require(challengeDigest.size == 32 && key.size == 32) { "Invalid approval ceremony" }
            val fields = listOf("filebeam-device-approval-v2", approval.instanceOrigin, approval.deploymentId, approval.accountId.toString(), approval.generation.toString(), approval.candidateDeviceId, approval.candidateKeyVersion.toString(), approval.approverDeviceId, approval.approverKeyVersion.toString(), approval.expiresAtUnix.toString())
            return identity.sign(fields.joinToString("\u0000", postfix = "\u0000").toByteArray() + challengeDigest + key)
        }
    }

    /** Rust supplies the canonical payload only after validating the typed request. */
    private class PeerSigner(private val identity: DeviceIdentity) : NativeDeviceSigner {
        override fun signPeerProof(request: DevicePeerProofRequest): ByteArray = identity.sign(request.signingPayload)
        override fun signPeerReceipt(request: DeviceReceiptRequest): ByteArray = identity.sign(request.signingPayload)
    }
}
