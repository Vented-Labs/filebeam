package io.filebeam.android.platform.services

import org.json.JSONObject

data class Contact(val id: Long, val username: String, val status: String, val canSend: Boolean?, val autoDownload: Boolean?, val effectiveSend: Boolean, val effectiveDownload: Boolean)
data class ContactsState(val policy: String, val autoDownloadFriends: Boolean, val contacts: List<Contact>, val blocked: List<String>) {
    companion object {
        fun decode(value: String): ContactsState {
            val json = JSONObject(value)
            val settings = json.getJSONObject("settings")
            val contacts = json.getJSONArray("contacts")
            val blocks = json.getJSONArray("blocked")
            return ContactsState(settings.getString("receivingPolicy"), settings.getBoolean("autoDownloadFriends"),
                (0 until contacts.length()).map { index ->
                    val item = contacts.getJSONObject(index)
                    val effective = item.getJSONObject("effective")
                    Contact(item.getLong("id"), item.getString("username"), item.getString("status"), item.optionalBoolean("canSend"), item.optionalBoolean("autoDownload"), effective.getBoolean("canSend"), effective.getBoolean("autoDownload"))
                }, (0 until blocks.length()).map { blocks.getJSONObject(it).getString("username") })
        }
    }
}
private fun JSONObject.optionalBoolean(name: String): Boolean? = if (isNull(name)) null else getBoolean(name)
