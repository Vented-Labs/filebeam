import Foundation

/// Syntax roles, matching the web editor's colour tokens.
public enum TokenKind: String, Sendable, CaseIterable {
    case keyword, string, number, constant, function, property, type, tag, attribute, comment, punctuation, `operator`, meta, escape, link, heading, invalid
}

/// A coloured range of the source, as UTF-16 offsets.
public struct Token: Hashable, Sendable {
    public let range: NSRange
    public let kind: TokenKind
    public init(range: NSRange, kind: TokenKind) { self.range = range; self.kind = kind }
}

/// Languages a note can be labelled with; keys match the web and the wire format.
public enum NoteLanguageKey: String, CaseIterable, Sendable {
    case plain, php, dotenv, javascript, typescript, json, markdown, css, html

    public var label: String {
        switch self {
        case .plain: "Plain text"
        case .php: "PHP"
        case .dotenv: ".ENV"
        case .javascript: "JavaScript"
        case .typescript: "TypeScript"
        case .json: "JSON"
        case .markdown: "Markdown"
        case .css: "CSS"
        case .html: "HTML"
        }
    }

    public var fileExtension: String {
        switch self {
        case .plain: "txt"
        case .php: "php"
        case .dotenv: "env"
        case .javascript: "js"
        case .typescript: "ts"
        case .json: "json"
        case .markdown: "md"
        case .css: "css"
        case .html: "html"
        }
    }
}

private struct Rule: @unchecked Sendable {
    let regex: NSRegularExpression
    let kind: TokenKind?

    init(_ pattern: String, _ kind: TokenKind?, options: NSRegularExpression.Options = []) {
        // Patterns are fixed strings reviewed with the web editor; a compile failure is a programming error.
        regex = try! NSRegularExpression(pattern: pattern, options: options)
        self.kind = kind
    }
}

/// A small ordered-rule lexer per language. It favours speed and stable colouring over full
/// grammar coverage: each rule is tried at the cursor and the first match wins.
public enum Highlighter {
    /// Above this size highlighting is skipped, as the web editor does, to keep typing fluid.
    public static let maximumHighlightCharacters = 250_000

    private static let identifier = #"[A-Za-z_$][\w$]*"#
    private static let jsKeywords = "abstract|as|async|await|break|case|catch|class|const|continue|debugger|declare|default|delete|do|else|enum|export|extends|finally|for|from|function|get|if|implements|import|in|instanceof|interface|is|keyof|let|module|namespace|new|of|package|private|protected|public|readonly|return|set|static|super|switch|this|throw|try|type|typeof|var|void|while|with|yield"
    private static let jsTypes = "any|bigint|boolean|never|number|object|string|symbol|unknown"

    private static func script(typed: Bool) -> [Rule] {
        [
            Rule(#"//[^\n]*|/\*[\s\S]*?(?:\*/|$)"#, .comment),
            Rule(#""(?:\\.|[^"\\\n])*"?|'(?:\\.|[^'\\\n])*'?|`(?:\\[\s\S]|[^`\\])*`?"#, .string),
            Rule(#"\b(?:0[xX][\da-fA-F_]+|0[bB][01_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)n?\b"#, .number),
            Rule(#"\b(?:true|false|null|undefined|NaN|Infinity)\b"#, .constant),
            Rule(#"\b(?:"# + jsKeywords + #")\b"#, .keyword),
            Rule(typed ? #"\b(?:"# + jsTypes + #")\b|\b[A-Z][\w$]*\b"# : #"\b[A-Z][\w$]*\b"#, .type),
            Rule(identifier + #"(?=\s*\()"#, .function),
            Rule(#"(?<=\.)"# + identifier, .property),
            Rule(identifier + #"(?=\s*:)"#, .property),
            Rule(identifier, nil),
            Rule(#"=>|[+\-*/%=!<>&|^~?]+"#, .operator),
            Rule(#"[{}()\[\];,.:]"#, .punctuation),
        ]
    }

    private static let json: [Rule] = [
        Rule(#""(?:\\.|[^"\\\n])*"(?=\s*:)"#, .property),
        Rule(#""(?:\\.|[^"\\\n])*"?"#, .string),
        Rule(#"-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b"#, .number),
        Rule(#"\b(?:true|false|null)\b"#, .constant),
        Rule(#"[{}\[\],:]"#, .punctuation),
        Rule(#"[^\s{}\[\],:"]+"#, .invalid),
    ]

    private static let php: [Rule] = [
        Rule(#"<\?(?:php|=)?|\?>"#, .meta),
        Rule(#"//[^\n]*|#(?!\[)[^\n]*|/\*[\s\S]*?(?:\*/|$)"#, .comment),
        Rule(#""(?:\\.|[^"\\])*"?|'(?:\\.|[^'\\])*'?"#, .string),
        Rule(#"\$[A-Za-z_]\w*"#, .property),
        Rule(#"\b(?:0[xX][\da-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)\b"#, .number),
        Rule(#"\b(?:true|false|null)\b"#, .constant, options: .caseInsensitive),
        Rule(#"\b(?:abstract|and|array|as|break|callable|case|catch|class|clone|const|continue|declare|default|do|echo|else|elseif|empty|enum|extends|final|finally|fn|for|foreach|function|global|if|implements|include|include_once|instanceof|insteadof|interface|isset|list|match|namespace|new|or|print|private|protected|public|readonly|require|require_once|return|static|switch|throw|trait|try|unset|use|var|while|xor|yield)\b"#, .keyword),
        Rule(#"\b(?:int|float|string|bool|void|mixed|never|object|iterable|self|parent)\b|\b[A-Z]\w*\b"#, .type),
        Rule(#"[A-Za-z_]\w*(?=\s*\()"#, .function),
        Rule(#"(?<=->|::)[A-Za-z_]\w*"#, .property),
        Rule(#"[A-Za-z_]\w*"#, nil),
        Rule(#"=>|->|::|[+\-*/%=!<>&|^~?.]+"#, .operator),
        Rule(#"[{}()\[\];,:]"#, .punctuation),
    ]

    private static let dotenv: [Rule] = [
        Rule(#"#[^\n]*"#, .comment),
        Rule(#"(?m)^\s*(?:export\s+)?[A-Za-z_][\w.]*(?=\s*=)"#, .property),
        Rule("=", .operator),
        Rule(#""(?:\\.|[^"\\])*"?|'[^']*'?"#, .string),
        Rule(#"\$\{[^}\n]*\}|\$[A-Za-z_]\w*"#, .escape),
        Rule(#"\b(?:true|false|null)\b"#, .constant, options: .caseInsensitive),
        Rule(#"\b\d+(?:\.\d+)?\b"#, .number),
        Rule(#"[^\s#="'$]+"#, .string),
    ]

    private static let markdown: [Rule] = [
        Rule(#"(?m)^```[^\n]*\n[\s\S]*?(?:^```[^\n]*$|\z)"#, .string),
        Rule(#"(?m)^ {0,3}#{1,6}\s[^\n]*"#, .heading),
        Rule(#"(?m)^ {0,3}>[^\n]*"#, .comment),
        Rule(#"(?m)^\s*(?:[-*+]|\d+[.)])(?=\s)"#, .punctuation),
        Rule(#"`[^`\n]*`?"#, .string),
        Rule(#"!?\[[^\]\n]*\]\([^)\n]*\)"#, .link),
        Rule(#"https?://[^\s)>\]]+"#, .link),
        Rule(#"\*\*[^*\n]+\*\*|__[^_\n]+__"#, .keyword),
        Rule(#"\*[^*\s][^*\n]*\*|_[^_\s][^_\n]*_"#, .type),
        Rule(#"(?m)^ {0,3}(?:-{3,}|\*{3,}|_{3,})\s*$"#, .meta),
    ]

    private static let css: [Rule] = [
        Rule(#"/\*[\s\S]*?(?:\*/|$)"#, .comment),
        Rule(#""(?:\\.|[^"\\\n])*"?|'(?:\\.|[^'\\\n])*'?"#, .string),
        Rule(#"@[\w-]+"#, .keyword),
        Rule(#"#[\da-fA-F]{3,8}\b"#, .constant),
        Rule(#"-?\b\d*\.?\d+(?:%|[a-zA-Z]+)?"#, .number),
        Rule(#"--[\w-]+"#, .property),
        Rule(#"[\w-]+(?=\s*:(?![:\w-]*\s*\{))"#, .property),
        Rule(#"[\w-]+(?=\()"#, .function),
        Rule(#"[.#][\w-]+"#, .attribute),
        Rule(#"::?[\w-]+"#, .type),
        Rule(#"!important\b"#, .keyword),
        Rule(#"[\w-]+"#, .tag),
        Rule(#"[{}()\[\];,:>+~*=]"#, .punctuation),
    ]

    private static let html: [Rule] = [
        Rule(#"<!--[\s\S]*?(?:-->|$)"#, .comment),
        Rule(#"<![^>]*>?"#, .meta),
        Rule(#"(?<=</?)[A-Za-z][\w:-]*"#, .tag),
        Rule(#"</?|/?>"#, .punctuation),
        Rule(#""[^"]*"?|'[^']*'?"#, .string),
        Rule(#"[A-Za-z_:@][\w:.-]*(?=\s*=)"#, .attribute),
        Rule("=", .operator),
        Rule(#"&[#\w]+;"#, .escape),
    ]

    private static let whitespace = Rule(#"\s+"#, nil)

    private static let rules: [NoteLanguageKey: [Rule]] = [
        .javascript: script(typed: false), .typescript: script(typed: true), .json: json, .php: php,
        .dotenv: dotenv, .markdown: markdown, .css: css, .html: html,
    ]

    /// Returns coloured ranges for `text`, or none for plain text, unknown languages and oversized input.
    public static func highlight(_ text: String, language: String) -> [Token] {
        guard let key = NoteLanguageKey(rawValue: language), let set = rules[key] else { return [] }
        let source = text as NSString
        let length = source.length
        if length > maximumHighlightCharacters { return [] }
        var tokens: [Token] = []
        tokens.reserveCapacity(length / 6 + 8)
        var index = 0
        let ordered = [whitespace] + set
        while index < length {
            var advanced = false
            for rule in ordered {
                let range = NSRange(location: index, length: length - index)
                guard let match = rule.regex.firstMatch(in: text, options: .anchored, range: range), match.range.length > 0 else { continue }
                if let kind = rule.kind { tokens.append(Token(range: match.range, kind: kind)) }
                index += match.range.length
                advanced = true
                break
            }
            if !advanced { index += 1 }
        }
        return tokens
    }
}
