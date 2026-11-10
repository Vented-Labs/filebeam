package io.filebeam.android.ui.design.code

/** Syntax roles, matching the web editor's colour tokens. */
enum class TokenKind { Keyword, String, Number, Constant, Function, Property, Type, Tag, Attribute, Comment, Punctuation, Operator, Meta, Escape, Link, Heading, Invalid }

/** A coloured range of the source, as UTF-16 offsets. */
data class Token(val start: Int, val end: Int, val kind: TokenKind)

private class Rule(pattern: String, val kind: TokenKind?, val group: Int = 0, options: Set<RegexOption> = emptySet()) {
    val regex = Regex(pattern, options)
}

/**
 * A small ordered-rule lexer per language. It favours speed and stable colouring over full
 * grammar coverage: each rule is tried at the cursor and the first match wins.
 */
object Highlighter {
    /** Above this size highlighting is skipped, as the web editor does, to keep typing fluid. */
    const val MAX_HIGHLIGHT_CHARS = 250_000

    private const val IDENT = "[A-Za-z_$][\\w$]*"
    private val jsKeywords = "abstract|as|async|await|break|case|catch|class|const|continue|debugger|declare|default|delete|do|else|enum|export|extends|finally|for|from|function|get|if|implements|import|in|instanceof|interface|is|keyof|let|module|namespace|new|of|package|private|protected|public|readonly|return|set|static|super|switch|this|throw|try|type|typeof|var|void|while|with|yield"
    private val jsTypes = "any|bigint|boolean|never|number|object|string|symbol|unknown"

    private fun script(typed: Boolean) = listOf(
        Rule("//[^\\n]*|/\\*[\\s\\S]*?(?:\\*/|$)", TokenKind.Comment),
        Rule("\"(?:\\\\.|[^\"\\\\\\n])*\"?|'(?:\\\\.|[^'\\\\\\n])*'?|`(?:\\\\[\\s\\S]|[^`\\\\])*`?", TokenKind.String),
        Rule("\\b(?:0[xX][\\da-fA-F_]+|0[bB][01_]+|\\d[\\d_]*(?:\\.\\d+)?(?:[eE][+-]?\\d+)?)n?\\b", TokenKind.Number),
        Rule("\\b(?:true|false|null|undefined|NaN|Infinity)\\b", TokenKind.Constant),
        Rule("\\b(?:$jsKeywords)\\b", TokenKind.Keyword),
        Rule(if (typed) "\\b(?:$jsTypes)\\b|\\b[A-Z][\\w$]*\\b" else "\\b[A-Z][\\w$]*\\b", TokenKind.Type),
        Rule("$IDENT(?=\\s*\\()", TokenKind.Function),
        Rule("(?<=\\.)$IDENT", TokenKind.Property),
        Rule("$IDENT(?=\\s*:)", TokenKind.Property),
        Rule(IDENT, null),
        Rule("=>|[+\\-*/%=!<>&|^~?]+", TokenKind.Operator),
        Rule("[{}()\\[\\];,.:]", TokenKind.Punctuation),
    )

    private val json = listOf(
        Rule("\"(?:\\\\.|[^\"\\\\\\n])*\"(?=\\s*:)", TokenKind.Property),
        Rule("\"(?:\\\\.|[^\"\\\\\\n])*\"?", TokenKind.String),
        Rule("-?\\b\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?\\b", TokenKind.Number),
        Rule("\\b(?:true|false|null)\\b", TokenKind.Constant),
        Rule("[{}\\[\\],:]", TokenKind.Punctuation),
        Rule("[^\\s{}\\[\\],:\"]+", TokenKind.Invalid),
    )

    private val php = listOf(
        Rule("<\\?(?:php|=)?|\\?>", TokenKind.Meta),
        Rule("//[^\\n]*|#(?!\\[)[^\\n]*|/\\*[\\s\\S]*?(?:\\*/|$)", TokenKind.Comment),
        Rule("\"(?:\\\\.|[^\"\\\\])*\"?|'(?:\\\\.|[^'\\\\])*'?", TokenKind.String),
        Rule("\\$[A-Za-z_]\\w*", TokenKind.Property),
        Rule("\\b(?:0[xX][\\da-fA-F_]+|\\d[\\d_]*(?:\\.\\d+)?(?:[eE][+-]?\\d+)?)\\b", TokenKind.Number),
        Rule("\\b(?:true|false|null)\\b", TokenKind.Constant, options = setOf(RegexOption.IGNORE_CASE)),
        Rule("\\b(?:abstract|and|array|as|break|callable|case|catch|class|clone|const|continue|declare|default|do|echo|else|elseif|empty|enum|extends|final|finally|fn|for|foreach|function|global|if|implements|include|include_once|instanceof|insteadof|interface|isset|list|match|namespace|new|or|print|private|protected|public|readonly|require|require_once|return|static|switch|throw|trait|try|unset|use|var|while|xor|yield)\\b", TokenKind.Keyword),
        Rule("\\b(?:int|float|string|bool|void|mixed|never|object|iterable|self|parent)\\b|\\b[A-Z]\\w*\\b", TokenKind.Type),
        Rule("[A-Za-z_]\\w*(?=\\s*\\()", TokenKind.Function),
        Rule("(?<=->|::)[A-Za-z_]\\w*", TokenKind.Property),
        Rule("[A-Za-z_]\\w*", null),
        Rule("=>|->|::|[+\\-*/%=!<>&|^~?.]+", TokenKind.Operator),
        Rule("[{}()\\[\\];,:]", TokenKind.Punctuation),
    )

    private val dotenv = listOf(
        Rule("#[^\\n]*", TokenKind.Comment),
        Rule("(?m)^\\s*(?:export\\s+)?[A-Za-z_][\\w.]*(?=\\s*=)", TokenKind.Property),
        Rule("=", TokenKind.Operator),
        Rule("\"(?:\\\\.|[^\"\\\\])*\"?|'[^']*'?", TokenKind.String),
        Rule("\\$\\{[^}\\n]*\\}|\\$[A-Za-z_]\\w*", TokenKind.Escape),
        Rule("\\b(?:true|false|null)\\b", TokenKind.Constant, options = setOf(RegexOption.IGNORE_CASE)),
        Rule("\\b\\d+(?:\\.\\d+)?\\b", TokenKind.Number),
        Rule("[^\\s#=\"'$]+", TokenKind.String),
    )

    private val markdown = listOf(
        Rule("(?m)^```[^\\n]*\\n[\\s\\S]*?(?:^```[^\\n]*$|\\z)", TokenKind.String),
        Rule("(?m)^ {0,3}#{1,6}\\s[^\\n]*", TokenKind.Heading),
        Rule("(?m)^ {0,3}>[^\\n]*", TokenKind.Comment),
        Rule("(?m)^\\s*(?:[-*+]|\\d+[.)])(?=\\s)", TokenKind.Punctuation),
        Rule("`[^`\\n]*`?", TokenKind.String),
        Rule("!?\\[[^\\]\\n]*\\]\\([^)\\n]*\\)", TokenKind.Link),
        Rule("https?://[^\\s)>\\]]+", TokenKind.Link),
        Rule("\\*\\*[^*\\n]+\\*\\*|__[^_\\n]+__", TokenKind.Keyword),
        Rule("\\*[^*\\s][^*\\n]*\\*|_[^_\\s][^_\\n]*_", TokenKind.Type),
        Rule("(?m)^ {0,3}(?:-{3,}|\\*{3,}|_{3,})\\s*$", TokenKind.Meta),
    )

    private val css = listOf(
        Rule("/\\*[\\s\\S]*?(?:\\*/|$)", TokenKind.Comment),
        Rule("\"(?:\\\\.|[^\"\\\\\\n])*\"?|'(?:\\\\.|[^'\\\\\\n])*'?", TokenKind.String),
        Rule("@[\\w-]+", TokenKind.Keyword),
        Rule("#[\\da-fA-F]{3,8}\\b", TokenKind.Constant),
        Rule("-?\\b\\d*\\.?\\d+(?:%|[a-zA-Z]+)?", TokenKind.Number),
        Rule("--[\\w-]+", TokenKind.Property),
        Rule("[\\w-]+(?=\\s*:(?![:\\w-]*\\s*\\{))", TokenKind.Property),
        Rule("[\\w-]+(?=\\()", TokenKind.Function),
        Rule("[.#][\\w-]+", TokenKind.Attribute),
        Rule("::?[\\w-]+", TokenKind.Type),
        Rule("!important\\b", TokenKind.Keyword),
        Rule("[\\w-]+", TokenKind.Tag),
        Rule("[{}()\\[\\];,:>+~*=]", TokenKind.Punctuation),
    )

    private val html = listOf(
        Rule("<!--[\\s\\S]*?(?:-->|$)", TokenKind.Comment),
        Rule("<![^>]*>?", TokenKind.Meta),
        Rule("(?<=</?)[A-Za-z][\\w:-]*", TokenKind.Tag),
        Rule("</?|/?>", TokenKind.Punctuation),
        Rule("\"[^\"]*\"?|'[^']*'?", TokenKind.String),
        Rule("[A-Za-z_:@][\\w:.-]*(?=\\s*=)", TokenKind.Attribute),
        Rule("=", TokenKind.Operator),
        Rule("&[#\\w]+;", TokenKind.Escape),
    )

    private val rules: Map<String, List<Rule>> by lazy {
        mapOf(
            "javascript" to script(false), "typescript" to script(true), "json" to json, "php" to php,
            "dotenv" to dotenv, "markdown" to markdown, "css" to css, "html" to html,
        )
    }

    /** Returns coloured ranges for [text], or none for plain text, unknown languages and oversized input. */
    fun highlight(text: String, language: String): List<Token> {
        val set = rules[language] ?: return emptyList()
        if (text.length > MAX_HIGHLIGHT_CHARS) return emptyList()
        val tokens = ArrayList<Token>(text.length / 6 + 8)
        var index = 0
        while (index < text.length) {
            if (text[index].isWhitespace()) {
                index++
                continue
            }
            var advanced = false
            for (rule in set) {
                val match = rule.regex.matchAt(text, index) ?: continue
                if (match.value.isEmpty()) continue
                rule.kind?.let { tokens += Token(index, index + match.value.length, it) }
                index += match.value.length
                advanced = true
                break
            }
            if (!advanced) index++
        }
        return tokens
    }
}
