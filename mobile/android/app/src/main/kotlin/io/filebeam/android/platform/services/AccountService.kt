package io.filebeam.android.platform.services

import android.content.Context
import android.util.Base64
import io.filebeam.android.platform.security.PendingTransferStore
import io.filebeam.rust.AccountKeyUpload
import io.filebeam.rust.NativeServices
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.URI
import java.security.MessageDigest

data class AccountSummary(val id: ULong, val username: String, val instance: String, val inboxEnabled: Boolean)

interface AccountService {
    val state: StateFlow<ServiceState<AccountSummary>>
    suspend fun signIn(instance: String, username: String, password: String)
    suspend fun signUp(instance: String, username: String, name: String?, email: String, password: String)
    suspend fun resume(instance: String)
    suspend fun signOut()
    suspend fun generateKey(): String
    suspend fun exportKey(): String
    suspend fun importKey(value: String)
    suspend fun privateKeyForInbox(bundleId: ULong, userId: ULong, custodyMode: String, envelope: String?, publicKey: String, password: String?): ByteArray
    suspend fun rememberInboxKey(bundleId: ULong, privateKey: ByteArray)
}

/** One process owns each origin's Rust cookie jar so account, inbox, and recipient lookup agree. */
class AccountSessionRegistry(private val allowHttp: Boolean) {
    private val services = mutableMapOf<String, NativeServices>()
    private val cookies = mutableMapOf<String, String>()

    @Synchronized fun service(instance: String): NativeServices = services.getOrPut(normalizeOrigin(instance)) {
        NativeServices(normalizeOrigin(instance), allowHttp)
    }
    @Synchronized fun cookie(instance: String): String? = cookies[normalizeOrigin(instance)]
    @Synchronized fun authenticated(instance: String, cookie: String) { cookies[normalizeOrigin(instance)] = cookie }
    @Synchronized fun restore(instance: String, cookie: String) {
        val origin = normalizeOrigin(instance)
        // Existing calls may still be using the previous jar. Let it finish rather than destroying it.
        services[origin] = NativeServices.newWithCookieContext(origin, allowHttp, cookie)
        cookies[origin] = cookie
    }
    @Synchronized fun clear(instance: String) { cookies.remove(normalizeOrigin(instance)) }

    companion object {
        fun normalizeOrigin(value: String): String {
            val uri = URI(value.trim())
            val scheme = uri.scheme?.lowercase()
            require(scheme in setOf("https", "http") && !uri.host.isNullOrBlank() && uri.userInfo == null && uri.query == null && uri.fragment == null) {
                "Enter a valid Filebeam instance origin"
            }
            val port = if (uri.port == -1 || uri.port == uri.toURL().defaultPort) "" else ":${uri.port}"
            return "$scheme://${uri.host.lowercase()}$port"
        }
    }
}

class NativeAccountService(
    private val context: Context,
    private val sessions: AccountSessionRegistry,
) : AccountService {
    private val mutable = MutableStateFlow<ServiceState<AccountSummary>>(ServiceState.Loading)
    override val state = mutable.asStateFlow()

    override suspend fun signIn(instance: String, username: String, password: String) = withContext(Dispatchers.IO) {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val session = sessions.service(origin).accountLogin(username, password, true)
        recoverPasswordCustody(origin, session.id, password)
        completeSession(origin, session.id, session.username ?: session.name, session.inboxEnabled)
    }

    override suspend fun signUp(instance: String, username: String, name: String?, email: String, password: String) = withContext(Dispatchers.IO) {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val session = sessions.service(origin).accountRegister(username, name?.takeIf(String::isNotBlank), email, password)
        recoverPasswordCustody(origin, session.id, password)
        completeSession(origin, session.id, session.username ?: session.name, session.inboxEnabled)
    }

    override suspend fun resume(instance: String) = withContext(Dispatchers.IO) {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val persisted = sessionStore(origin).load() ?: return@withContext
        val cookie = persisted.optString("cookie").takeIf(String::isNotBlank) ?: return@withContext
        sessions.restore(origin, cookie)
        val session = sessions.service(origin).accountSession()
        completeSession(origin, session.id, session.username ?: session.name, session.inboxEnabled)
    }

    override suspend fun signOut() = withContext(Dispatchers.IO) {
        val account = requireAccount()
        sessions.service(account.instance).accountLogout()
        sessions.clear(account.instance)
        sessionStore(account.instance).clear()
        mutable.value = ServiceState.Loading
    }

    override suspend fun generateKey(): String = withContext(Dispatchers.IO) {
        val account = requireAccount()
        val service = sessions.service(account.instance)
        val generated = service.accountGenerateSelfKey()
        commitGeneratedKey(generated.privateKey,
            upload = { service.accountUploadKey(AccountKeyUpload(generated.publicKey, generated.fingerprint, "self", null, null, true)).id },
            export = service::accountExportSelfKey,
        ) { bundle, key ->
            keyStore(account, bundle).save(JSONObject().put("key", Base64.encodeToString(key, Base64.NO_WRAP)))
            selectBundle(account, bundle)
        }
    }

    override suspend fun exportKey(): String = withContext(Dispatchers.IO) {
        val account = requireAccount()
        sessions.service(account.instance).accountExportSelfKey(loadPrivate(account))
    }

    override suspend fun importKey(value: String): Unit = withContext(Dispatchers.IO) {
        val account = requireAccount()
        val privateKey = sessions.service(account.instance).accountImportSelfKey(value)
        try {
            // A historical fbsk1 key is associated only after native inbox open validates it against a bundle.
            importedKeyStore(account).save(JSONObject().put("key", Base64.encodeToString(privateKey, Base64.NO_WRAP)))
        } finally {
            privateKey.fill(0)
        }
    }

    override suspend fun privateKeyForInbox(bundleId: ULong, userId: ULong, custodyMode: String, envelope: String?, publicKey: String, password: String?): ByteArray = withContext(Dispatchers.IO) {
        val account = requireAccount()
        check(account.id == userId) { "This inbox key belongs to another account" }
        keyStore(account, bundleId).load()?.optString("key")?.let { Base64.decode(it, Base64.NO_WRAP) }
            ?.takeIf { it.size == 32 }?.let { return@withContext it }
        if (custodyMode == "password" && envelope != null && !password.isNullOrEmpty()) {
            val key = sessions.service(account.instance).accountUnwrapPasswordKey(envelope, password, userId, publicKey)
            require(key.size == 32) { "Invalid account private key" }
            return@withContext key
        }
        importedKeyStore(account).load()?.optString("key")?.let { Base64.decode(it, Base64.NO_WRAP) }
            ?.takeIf { it.size == 32 } ?: error(if (custodyMode == "password") "Enter your account password or import the matching fbsk1 key" else "Import the fbsk1 key for this inbox delivery")
    }

    override suspend fun rememberInboxKey(bundleId: ULong, privateKey: ByteArray) = withContext(Dispatchers.IO) {
        val account = requireAccount()
        require(privateKey.size == 32) { "Invalid account private key" }
        keyStore(account, bundleId).save(JSONObject().put("key", Base64.encodeToString(privateKey, Base64.NO_WRAP)))
        importedKeyStore(account).clear()
    }

    private fun completeSession(origin: String, id: ULong, username: String, inboxEnabled: Boolean) {
        val cookie = sessions.service(origin).accountCookieContext()
        sessions.authenticated(origin, cookie)
        sessionStore(origin).save(JSONObject().put("cookie", cookie).put("user", id.toString()).put("username", username))
        mutable.value = ServiceState.Ready(AccountSummary(id, username, origin, inboxEnabled))
    }
    private fun recoverPasswordCustody(origin: String, userId: ULong, password: String) {
        val service = sessions.service(origin)
        service.accountKeys().filter { it.isActive && it.custodyMode == "password" && it.encryptedPrivateKey != null }.forEach { bundle ->
            val privateKey = service.accountUnwrapPasswordKey(bundle.encryptedPrivateKey!!, password, userId, bundle.publicKey)
            try {
                val account = AccountSummary(userId, "", origin, false)
                keyStore(account, bundle.id).save(JSONObject().put("key", Base64.encodeToString(privateKey, Base64.NO_WRAP)))
                selectBundle(account, bundle.id)
            } finally {
                privateKey.fill(0)
            }
        }
    }
    private fun requireAccount() = (state.value as? ServiceState.Ready)?.value ?: error("Sign in first")
    private fun sessionStore(origin: String) = PendingTransferStore(context, "account-session-${digest(origin)}")
    private fun selectionStore(account: AccountSummary) = PendingTransferStore(context, "account-key-selection-${digest("${account.instance}:${account.id}")}")
    private fun keyStore(account: AccountSummary, bundle: ULong) = PendingTransferStore(context, "account-private-${digest("${account.instance}:${account.id}:$bundle")}")
    private fun importedKeyStore(account: AccountSummary) = PendingTransferStore(context, "account-private-import-${digest("${account.instance}:${account.id}")}")
    private fun selectBundle(account: AccountSummary, bundle: ULong) = selectionStore(account).save(JSONObject().put("bundle", bundle.toString()))
    private fun loadPrivate(account: AccountSummary): ByteArray {
        val bundle = selectionStore(account).load()?.optString("bundle")?.toULongOrNull() ?: error("Import or generate an account key first")
        return keyStore(account, bundle).load()?.optString("key")?.let { Base64.decode(it, Base64.NO_WRAP) }
            ?.takeIf { it.size == 32 } ?: error("Import or generate an account key first")
    }
    private fun digest(value: String): String = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }
}

/** Keeps the only mutable copy valid until every durable step has acknowledged it. */
internal fun commitGeneratedKey(
    privateKey: ByteArray,
    upload: () -> ULong,
    export: (ByteArray) -> String,
    persist: (ULong, ByteArray) -> Unit,
): String = try {
    val bundle = upload()
    val exported = export(privateKey)
    persist(bundle, privateKey)
    exported
} finally {
    privateKey.fill(0)
}
