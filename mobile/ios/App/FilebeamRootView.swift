import SwiftUI
import UIKit
import FilebeamDomain

struct FilebeamRootView: View {
    @Bindable var model: AppModel
    let importSources: ([URL]) async -> [FileSource]
    @State private var draftSaveTask: Task<Void, Never>?

    var body: some View {
        TabView(selection: $model.selectedTab) {
            SendComposerView(model: model, importSources: importSources).tabItem { Label("Send", systemImage: "arrow.up.circle") }.tag(AppModel.Tab.send)
            ReceiveView(model: model).tabItem { Label("Receive", systemImage: "arrow.down.circle") }.tag(AppModel.Tab.receive)
            TransfersView(model: model).tabItem { Label("Transfers", systemImage: "arrow.left.arrow.right") }.tag(AppModel.Tab.transfers)
            InboxView(model: model).tabItem { Label("Inbox", systemImage: "tray") }.tag(AppModel.Tab.inbox)
            SettingsView(model: model).tabItem { Label("Settings", systemImage: "gearshape") }.tag(AppModel.Tab.settings)
        }
        .tint(.filebeamViolet)
        .task {
            await model.load()
            model.pollLiveJobs()
            if let paths = try? AppPaths(), let group = paths.group {
                await model.processPendingSharedDrafts(root: group, importer: importSources)
            }
        }
        .onChange(of: model.drafts) { _, drafts in
            draftSaveTask?.cancel()
            draftSaveTask = Task {
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled else { return }
                await model.persistDrafts(drafts)
            }
        }
        .onDisappear { draftSaveTask?.cancel() }
        .sheet(item: $model.activePrompt) { PromptSheet(model: model, context: $0) }
        .sheet(item: Binding(get: { model.externalRequests.active }, set: { _ in })) { request in ExternalRequestSheet(model: model, request: request) }
        .sheet(isPresented: Binding(get: { model.receipt != nil }, set: { if !$0 { model.receipt = nil } })) {
            if let receipt = model.receipt { ShareReceiptView(receipt: receipt) }
        }
        .sheet(isPresented: Binding(get: { model.downloadCommand != nil }, set: { if !$0 { model.downloadCommand = nil } })) {
            if let command = model.downloadCommand { DownloadCommandView(command: command) }
        }
    }
}

private struct DownloadCommandView: View {
    @Environment(\.dismiss) private var dismiss
    let command: String
    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text("Download with the Filebeam CLI").font(.title2.bold())
                Text(command).font(.system(.body, design: .monospaced)).textSelection(.enabled)
                Button("Copy command") { UIPasteboard.general.string = command }.buttonStyle(.borderedProminent)
                Spacer()
            }.padding().navigationTitle("CLI download").toolbar { Button("Done") { dismiss() } }
        }
    }
}

private struct ExternalRequestSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel
    let request: ExternalRequest
    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text(title).font(.title2.bold())
                Text(detail).foregroundStyle(.secondary)
                if case .sharedFiles = request { Button("Add to Send draft") { model.acceptStagedSharedFiles(); dismiss() }.buttonStyle(.borderedProminent) }
                else if case .sharedText = request {
                    Button("Append to note") { model.mergeStagedSharedText(replacing: false); dismiss() }.buttonStyle(.borderedProminent)
                    Button("Replace note") { model.mergeStagedSharedText(replacing: true); dismiss() }.buttonStyle(.bordered)
                } else { Button("Continue") { Task { await model.stageExternalRequest(); dismiss() } }.buttonStyle(.borderedProminent) }
                Spacer()
            }.padding().navigationTitle("Filebeam").toolbar { Button("Not now") { model.externalRequests.dismissActive(); dismiss() } }
        }.onAppear {
            switch request {
            case .sharedFiles, .sharedText: Task { await model.stageExternalRequest() }
            default: break
            }
        }
    }
    private var title: String { switch request { case .receive: return "Open transfer"; case .sharedFiles: return "Add shared files"; case .sharedText: return "Add shared text"; case .invite: return "Account invitation"; case .verifyEmail: return "Verify email"; case .resetPassword: return "Reset password"; case .profile: return "Prepare recipient" } }
    private var detail: String { switch request { case .receive: return "The transfer link will be placed in Receive. Downloading starts only when you choose it."; case .sharedFiles: return "Add these prepared files to the current Send draft without replacing it."; case .sharedText: return "Choose whether this shared text is added to or replaces the current note draft."; case .invite: return "Review and accept this invitation in Account."; case .verifyEmail: return "Verify this email address in Account."; case .resetPassword: return "Continue to the password reset form."; case .profile: return "The recipient will be validated before it is added to Send." } }
}
