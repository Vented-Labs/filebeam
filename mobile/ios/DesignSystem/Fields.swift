import SwiftUI

/// A labelled single- or multi-line input. The border warms to the accent on focus and to the
/// danger colour while `error` is set; the error message appears beneath.
struct FbTextField<Trailing: View>: View {
    @Environment(\.theme) private var theme
    @Binding var text: String
    var label: String?
    var placeholder: String?
    var supporting: String?
    var error: String?
    var icon: Icon?
    var secure = false
    var multiline = false
    var minLines = 1
    var monospaced = false
    var enabled = true
    var keyboard: UIKeyboardType = .default
    var contentType: UITextContentType?
    var autocapitalization: TextInputAutocapitalization = .never
    var submitLabel: SubmitLabel = .done
    var onSubmit: (() -> Void)?
    @ViewBuilder var trailing: () -> Trailing
    @FocusState private var focused: Bool

    init(_ text: Binding<String>, label: String? = nil, placeholder: String? = nil, supporting: String? = nil, error: String? = nil, icon: Icon? = nil, secure: Bool = false, multiline: Bool = false, minLines: Int = 1, monospaced: Bool = false, enabled: Bool = true, keyboard: UIKeyboardType = .default, contentType: UITextContentType? = nil, autocapitalization: TextInputAutocapitalization = .never, submitLabel: SubmitLabel = .done, onSubmit: (() -> Void)? = nil, @ViewBuilder trailing: @escaping () -> Trailing) {
        _text = text; self.label = label; self.placeholder = placeholder; self.supporting = supporting; self.error = error; self.icon = icon; self.secure = secure; self.multiline = multiline; self.minLines = minLines; self.monospaced = monospaced; self.enabled = enabled; self.keyboard = keyboard; self.contentType = contentType; self.autocapitalization = autocapitalization; self.submitLabel = submitLabel; self.onSubmit = onSubmit; self.trailing = trailing
    }

    var body: some View {
        let borderColor: Color = error != nil ? theme.danger.border : focused ? theme.accentText : !enabled ? theme.disabledBorder : theme.controlBorder
        VStack(alignment: .leading, spacing: Space.sm) {
            if let label { Text(label).fbText(.smallStrong, color: enabled ? theme.text : theme.disabledText) }
            HStack(alignment: multiline ? .top : .center, spacing: Space.md) {
                if let icon { FbIcon(icon, size: 20, tint: focused ? theme.accentText : theme.textMuted).padding(.top, multiline ? 2 : 0) }
                field
                    .focused($focused)
                    .fbText(monospaced ? .code : .body, color: enabled ? theme.text : theme.disabledText)
                    .tint(theme.accentText)
                    .keyboardType(keyboard)
                    .textContentType(contentType)
                    .textInputAutocapitalization(autocapitalization)
                    .autocorrectionDisabled()
                    .submitLabel(submitLabel)
                    .onSubmit { onSubmit?() }
                    .disabled(!enabled)
                trailing()
            }
            .padding(.leading, Space.lg)
            .padding(.trailing, Trailing.self == EmptyView.self ? Space.lg : Space.xs)
            .padding(.vertical, multiline ? Space.md : 0)
            .frame(minHeight: Space.control)
            .background(enabled ? theme.sunken : theme.disabledSurface, in: Shapes.control)
            .overlay(Shapes.control.strokeBorder(borderColor, lineWidth: focused || error != nil ? 1.5 : 1))
            .motion(Motion.fade, value: borderColor)
            .contentShape(Shapes.control)
            .onTapGesture { focused = true }
            if let message = error ?? supporting {
                Text(message).fbText(.small, color: error != nil ? theme.danger.ink : theme.textMuted)
                    .fixedSize(horizontal: false, vertical: true)
                    .transition(.opacity.combined(with: .move(edge: .top)))
                    .accessibilityAddTraits(error != nil ? .isStaticText : [])
            }
        }
        .motion(Motion.fade, value: error)
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder private var field: some View {
        let prompt = placeholder.map { Text($0).foregroundStyle(theme.textSubtle) }
        if secure { SecureField("", text: $text, prompt: prompt) }
        else if multiline { TextField("", text: $text, prompt: prompt, axis: .vertical).lineLimit(minLines...max(minLines, 8)) }
        else { TextField("", text: $text, prompt: prompt) }
    }
}

extension FbTextField where Trailing == EmptyView {
    init(_ text: Binding<String>, label: String? = nil, placeholder: String? = nil, supporting: String? = nil, error: String? = nil, icon: Icon? = nil, secure: Bool = false, multiline: Bool = false, minLines: Int = 1, monospaced: Bool = false, enabled: Bool = true, keyboard: UIKeyboardType = .default, contentType: UITextContentType? = nil, autocapitalization: TextInputAutocapitalization = .never, submitLabel: SubmitLabel = .done, onSubmit: (() -> Void)? = nil) {
        self.init(text, label: label, placeholder: placeholder, supporting: supporting, error: error, icon: icon, secure: secure, multiline: multiline, minLines: minLines, monospaced: monospaced, enabled: enabled, keyboard: keyboard, contentType: contentType, autocapitalization: autocapitalization, submitLabel: submitLabel, onSubmit: onSubmit) { EmptyView() }
    }
}

/// A password input with a show/hide control. The value is never logged or persisted by this component.
struct PasswordField: View {
    @Binding var text: String
    var label: String?
    var placeholder: String?
    var supporting: String?
    var error: String?
    var enabled = true
    var contentType: UITextContentType = .password
    var submitLabel: SubmitLabel = .done
    var onSubmit: (() -> Void)?
    @State private var visible = false

    init(_ text: Binding<String>, label: String? = nil, placeholder: String? = nil, supporting: String? = nil, error: String? = nil, enabled: Bool = true, contentType: UITextContentType = .password, submitLabel: SubmitLabel = .done, onSubmit: (() -> Void)? = nil) {
        _text = text; self.label = label; self.placeholder = placeholder; self.supporting = supporting; self.error = error; self.enabled = enabled; self.contentType = contentType; self.submitLabel = submitLabel; self.onSubmit = onSubmit
    }

    var body: some View {
        FbTextField($text, label: label, placeholder: placeholder, supporting: supporting, error: error, icon: .key, secure: !visible, enabled: enabled, contentType: visible ? .none : contentType, submitLabel: submitLabel, onSubmit: onSubmit) {
            IconAction(visible ? .eyeOff : .eye, visible ? "Hide password" : "Show password", size: 36) { visible.toggle() }
        }
        .privacySensitive()
    }
}

/// A read-only value with an icon-only copy action, so the value itself gets the width.
/// `label` is the control's accessible label; the primary variant fills with the action colour.
struct CopyField: View {
    @Environment(\.theme) private var theme
    let icon: Icon
    let value: String
    let label: String
    var primary = false
    let onCopy: () -> Void

    var body: some View {
        HStack(spacing: Space.md) {
            FbIcon(icon, size: 18)
            Text(value).fbText(.codeSmall, color: theme.textMuted).lineLimit(1).truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
            Button {
                Haptics.tap()
                onCopy()
            } label: {
                FbIcon(.copy, size: 20, tint: primary ? theme.onAction : theme.text)
                    .frame(width: 44, height: 44)
                    .background(primary ? AnyShapeStyle(theme.actionGradient) : AnyShapeStyle(theme.surface), in: Shapes.control)
                    .overlay(Shapes.control.strokeBorder(primary ? theme.actionBorder : theme.controlBorder, lineWidth: 1))
            }
            .buttonStyle(.pressScale(0.9))
            .accessibilityLabel(label)
        }
        .padding(.leading, Space.md)
        .padding(.trailing, Space.xs)
        .padding(.vertical, Space.xs)
        .background(theme.sunken, in: Shapes.control)
        .overlay(Shapes.control.strokeBorder(theme.border, lineWidth: 1))
    }
}
