import SwiftUI
import UIKit

/// A chapter's verses `from`…`to`, typeset on their own.
struct Span: Hashable {
    var chapter: Int
    var from: Int
    var to: Int
}

extension ReadingPassage {
    /// Its verses chapter by chapter: John 13:31–18:1 is 13:31 on, 14 to 17 whole, then 18:1.
    var spans: [Span] {
        let first = Int(from.chapter)
        let last = max(Int(to.chapter), first)
        return (first...last).map { c in
            Span(
                chapter: Int(chapterIndex) + c - first, from: c == first ? Int(from.verse) : 1,
                to: c == last ? Int(to.verse) : 255)
        }
    }
}

/// Where a reading has got to: a span (counted across its parts) and the verse at the top.
struct PassagePlace: Equatable {
    var span: Int
    var verse: Int
}

/// A step's Scripture in the reader's type, size and settings (Orthodox fork): each part under
/// its label when there are several, typeset from its first verse to its last so nothing either
/// side of the reading shows, then its end line. Highlights and note markers are drawn. It
/// doesn't use `ReaderView`, whose coordinator moves the main reader.
struct PassageView<Footer: View>: View {
    @Environment(Store.self) private var store
    @Environment(\.dynamicTypeSize) private var dynamicType
    let parts: [RulePart]
    /// The line over the text: "2 Corinthians 9:6–11 · Luke 9:44–50".
    let intro: String?
    let theme: Theme
    /// Where it opens, and where it has got to as it scrolls.
    let place: PassagePlace?
    let reached: (PassagePlace, ChapterContent) -> Void
    @ViewBuilder let footer: Footer

    @State private var layouts: [[ChapterLayout]]?
    @State private var width: CGFloat = 0
    @State private var tracker = PassageTracker()
    @State private var waiting = false

    var body: some View {
        let style = typography
        let column = style.column
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if let intro {
                        Text(intro)
                            .font(Font(Fonts.literata(style.s(17).rounded())))
                            .foregroundStyle(Color(theme.ink2))
                            .padding(.top, style.s(20))
                            .frame(width: column.width, alignment: .leading)
                            .padding(.leading, column.x)
                    }
                    if let gospel = parts.indices.dropFirst().first(where: { parts[$0].label == "Gospel" }) {
                        Button {
                            withAnimation { proxy.scrollTo("part-\(gospel)", anchor: .top) }
                        } label: {
                            Label("Go to the Gospel", systemImage: "arrow.down")
                                .labelStyle(TrailingIcon())
                                .font(.subheadline.weight(.medium))
                                .frame(minHeight: 44)
                        }
                        .padding(.leading, column.x)
                        .accessibilityIdentifier("go-to-gospel")
                    }
                    if let layouts, layouts.count == parts.count {
                        text(layouts, style: style)
                    } else {
                        ProgressView()
                            .frame(maxWidth: .infinity, minHeight: 240)
                            .opacity(waiting ? 1 : 0)
                    }
                    footer
                        .frame(width: column.width)
                        .padding(.leading, column.x)
                        .padding(.top, style.s(24))
                        .padding(.bottom, 32)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .background(Color(theme.paper))
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .task(id: Typesetting(parts: parts, style: style, translation: store.settings.translation,
                              septuagint: store.settings.septuagintPsalms, marks: store.marksVersion))
        {
            await typeset(style)
        }
        .task {
            // A quiet page while a short reading typesets; a spinner only for a long one.
            try? await Task.sleep(for: .milliseconds(400))
            waiting = true
        }
        .accessibilityIdentifier("passage")
    }

    private var typography: Typography {
        var style = store.typography(theme, dynamicType: dynamicType)
        style.width = width
        return style
    }

    /// The parts' text: labels, spans, the gap and label between a reading's pieces, end lines.
    private func text(_ layouts: [[ChapterLayout]], style: Typography) -> some View {
        let column = style.column
        var first = 0
        let starts = layouts.map { spans in
            defer { first += spans.count }
            return first
        }
        return ForEach(parts.indices, id: \.self) { i in
            VStack(alignment: .leading, spacing: 0) {
                if parts.count > 1 {
                    label("\(parts[i].label) · \(parts[i].reference)", style: style)
                        .padding(.top, style.s(i == 0 ? 24 : 40))
                        .padding(.leading, column.x)
                        .id("part-\(i)")
                }
                let pieces = pieceStarts(parts[i])
                ForEach(layouts[i].indices, id: \.self) { j in
                    let layout = layouts[i][j]
                    if j > 0, let name = pieces[j], !layout.content.head {
                        label(name, style: style)
                            .padding(.top, style.s(16))
                            .padding(.leading, column.x)
                    }
                    SpanCanvasView(layout: layout, theme: theme, tracker: tracker, index: starts[i] + j)
                        .frame(height: layout.textBottom)
                        // A chapter or psalm of its own, as far below the last as the reader has it.
                        .padding(.top, j > 0 ? (layout.content.head ? style.s(56) : 0) : style.s(parts.count > 1 ? 12 : 24))
                }
                EndLine(text: parts[i].end, style: style, theme: theme)
                    .frame(width: column.width)
                    .padding(.leading, column.x)
                    .padding(.top, style.s(28))
            }
        }
    }

    /// The spans that start a piece of a reading in pieces ("Romans 14:19–23; 16:25–27"), each
    /// named for its label: "16:25", or "John 13:3" when the book changes.
    private func pieceStarts(_ part: RulePart) -> [Int: String] {
        var names: [Int: String] = [:]
        var at = 0
        var previous: ReadingPassage?
        for p in part.passages {
            if let previous {
                let psalm = p.from.book == 25
                names[at] =
                    psalm
                    ? String(rangeLabel(from: p.from, to: p.from).split(separator: ":")[0])
                    : p.from.book == previous.from.book
                        ? "\(p.from.chapter):\(p.from.verse)" : rangeLabel(from: p.from, to: p.from)
            }
            previous = p
            at += p.spans.count
        }
        return names
    }

    private func label(_ text: String, style: Typography) -> some View {
        let size = style.s(11).rounded()
        return Text(text.uppercased())
            .font(Font(Fonts.inter(size)))
            .kerning(size * 0.12)
            .foregroundStyle(Color(theme.accent))
            .lineSpacing(style.s(4))
            .frame(width: style.column.width, alignment: .leading)
    }

    private func typeset(_ style: Typography) async {
        guard style.width > 0 else { return }
        let bible = store.bible
        let translation = store.settings.translation
        let options = style.options
        let spans = parts.map { $0.passages.flatMap(\.spans) }
        // Typesetting again (new size, mode or width) keeps the verse at the top where it was.
        tracker.pending = layouts == nil ? place : tracker.last ?? place
        tracker.reached = reached
        let typeset = await withCheckedContinuation { done in
            PassageTracker.queue.async {
                done.resume(returning: spans.map { spans in
                    spans.map { s in
                        let content = bible.chapterSpan(
                            translation: translation, index: UInt16(s.chapter), from: UInt8(clamping: s.from),
                            to: UInt8(clamping: s.to), options: options)
                        let layout = ChapterLayout(content: content, typography: style)
                        layout.marks = Dictionary(
                            bible.marks(translation: translation, chapter: UInt16(s.chapter)).map { (Int($0.verse), $0) },
                            uniquingKeysWith: { a, _ in a })
                        return layout
                    }
                })
            }
        }
        guard !Task.isCancelled else { return }
        tracker.contents = typeset.flatMap { $0.map(\.content) }
        tracker.lines = [:]
        layouts = typeset
    }

    /// What the text is typeset from: a change to any of it typesets it again.
    private struct Typesetting: Equatable {
        let parts: [RulePart]
        let style: Typography
        let translation: String
        let septuagint: Bool
        let marks: Int
    }
}

/// "Go to the Gospel ↓".
private struct TrailingIcon: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            configuration.title
            configuration.icon
        }
    }
}

/// "── End of the Gospel ──", in the reader's type.
struct EndLine: View {
    let text: String
    let style: Typography
    let theme: Theme

    var body: some View {
        HStack(spacing: 12) {
            rule
            Text(text)
                .font(Font(Fonts.literata(style.s(16).rounded(), italic: true)))
                .foregroundStyle(Color(theme.ink2))
                .multilineTextAlignment(.center)
                .layoutPriority(1)
            rule
        }
        .frame(maxWidth: .infinity)
    }

    private var rule: some View {
        Rectangle().fill(Color(theme.separator)).frame(maxWidth: 40, maxHeight: 1)
    }
}

/// What the spans of a passage say as it scrolls, and where to open; not observed, so scrolling
/// redraws nothing in SwiftUI.
@MainActor
final class PassageTracker {
    /// Typesetting is a few milliseconds a chapter: off the main thread, as the reader does it.
    nonisolated static let queue = DispatchQueue(label: "dev.bible-rs.ios.passage", qos: .userInitiated)

    /// A place to scroll to once the spans are laid out.
    var pending: PassagePlace?
    var last: PassagePlace?
    /// The spans' chapters, in order.
    var contents: [ChapterContent] = []
    var reached: (PassagePlace, ChapterContent) -> Void = { _, _ in }
    /// Where the line under the bar falls in each span (from its top), how tall the span is, and
    /// the verse there.
    var lines: [Int: (line: CGFloat, height: CGFloat, verse: Int)] = [:]

    /// A span has moved: the reading is at the first span the line hasn't passed the end of, so
    /// with the Epistle's end and the Gospel's label under the bar, it's at the Gospel.
    func moved(_ span: Int, line: CGFloat, height: CGFloat, verse: Int) {
        lines[span] = (line, height, verse)
        guard let at = lines.keys.sorted().first(where: { lines[$0]!.line < lines[$0]!.height }) else { return }
        let place = PassagePlace(span: at, verse: lines[at]!.verse)
        guard place != last, contents.indices.contains(place.span) else { return }
        last = place
        reached(place, contents[place.span])
    }
}

/// One typeset span in SwiftUI.
struct SpanCanvasView: UIViewRepresentable {
    let layout: ChapterLayout
    let theme: Theme
    let tracker: PassageTracker
    let index: Int

    func makeUIView(context: Context) -> SpanCanvas { SpanCanvas() }

    func updateUIView(_ view: SpanCanvas, context: Context) {
        view.show(layout, theme: theme, tracker: tracker, index: index)
    }
}

/// A span drawn in tiles as it comes into view, as the reader draws its chapters: a long one
/// (Holy Friday's John 14) is taller than any one layer should be. It follows the scroll view
/// it sits in, so it can say which verse is at the top and open at one.
final class SpanCanvas: UIView {
    private var layout: ChapterLayout?
    private var theme = Theme.light
    private weak var tracker: PassageTracker?
    private var index = 0
    private weak var scroll: UIScrollView?
    private var watching: [NSKeyValueObservation] = []
    private var tiles: [Int: Tile] = [:]
    private var accessibilityCache: [UIAccessibilityElement]?
    private let tileHeight: CGFloat = 256

    init() {
        super.init(frame: .zero)
        isAccessibilityElement = false
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    func show(_ layout: ChapterLayout, theme: Theme, tracker: PassageTracker, index: Int) {
        self.tracker = tracker
        self.index = index
        guard layout !== self.layout || theme != self.theme else { return }
        self.layout = layout
        self.theme = theme
        backgroundColor = theme.paper
        accessibilityCache = nil
        for tile in tiles.values { tile.removeFromSuperview() }
        tiles.removeAll()
        setNeedsLayout()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        watching = []
        var view = superview
        while let v = view, !(v is UIScrollView) { view = v.superview }
        scroll = view as? UIScrollView
        guard let scroll else { return }
        watching = [
            scroll.observe(\.contentOffset) { [weak self] _, _ in MainActor.assumeIsolated { self?.scrolled() } },
            scroll.observe(\.bounds) { [weak self] _, _ in MainActor.assumeIsolated { self?.scrolled() } },
        ]
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        scrolled()
        guard let layout, let tracker, let place = tracker.pending, place.span == index, let scroll else { return }
        tracker.pending = nil
        // A span's first verse opens at its head; the reading's, at the top of the page.
        let first = place.verse <= (layout.verseTops.first?.verse ?? 1)
        guard index > 0 || !first else { return }
        // Once SwiftUI has sized the whole page: the verse just under the bar, as the reader opens.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.window != nil else { return }
            let top = first ? 0 : (layout.top(of: place.verse) ?? 0) - layout.typography.s(4)
            let y = self.convert(CGPoint(x: 0, y: top), to: scroll).y - scroll.adjustedContentInset.top
            let most = scroll.contentSize.height + scroll.adjustedContentInset.bottom - scroll.bounds.height
            scroll.contentOffset.y = max(-scroll.adjustedContentInset.top, min(y, most))
        }
    }

    /// Draws the tiles in or near view and reports the verse at the top.
    private func scrolled() {
        guard let layout, let scroll, bounds.height > 0 else { return }
        let visible = convert(scroll.bounds, from: scroll)
        let first = Int((max(visible.minY - tileHeight, 0) / tileHeight).rounded(.down))
        let last = Int((min(visible.maxY + tileHeight, bounds.height) / tileHeight).rounded(.down))
        for (k, tile) in tiles where k < first || k > last {
            tile.removeFromSuperview()
            tiles[k] = nil
        }
        if first <= last {
            for k in first...last where tiles[k] == nil && CGFloat(k) * tileHeight < bounds.height {
                let tile = Tile()
                tile.canvas = self
                tile.isOpaque = true
                tile.isUserInteractionEnabled = false
                tile.backgroundColor = theme.paper
                tile.frame = CGRect(x: 0, y: CGFloat(k) * tileHeight, width: bounds.width, height: tileHeight)
                addSubview(tile)
                tiles[k] = tile
            }
        }
        for tile in tiles.values where tile.frame.width != bounds.width {
            tile.frame.size.width = bounds.width
            tile.setNeedsDisplay()
        }
        // The line just under the navigation bar.
        let line = visible.minY + scroll.adjustedContentInset.top + layout.typography.s(12)
        tracker?.moved(index, line: line, height: bounds.height, verse: layout.verse(atY: max(line, 0)))
    }

    private final class Tile: UIView {
        weak var canvas: SpanCanvas?

        override func draw(_ rect: CGRect) {
            guard let canvas, let layout = canvas.layout, let ctx = UIGraphicsGetCurrentContext() else { return }
            ctx.translateBy(x: 0, y: -frame.minY)
            layout.draw(rect.offsetBy(dx: 0, dy: frame.minY), decorations: .init(theme: canvas.theme))
        }
    }

    // MARK: Accessibility

    /// A verse to VoiceOver, as the reader gives it.
    override var accessibilityElements: [Any]? {
        get {
            if let accessibilityCache { return accessibilityCache }
            guard let layout else { return [] }
            var elements: [UIAccessibilityElement] = []
            for heading in layout.headingElements {
                let e = UIAccessibilityElement(accessibilityContainer: self)
                e.accessibilityLabel = heading.text
                e.accessibilityTraits = .header
                e.accessibilityFrameInContainerSpace = heading.frame
                elements.append(e)
            }
            for top in layout.verseTops {
                guard let text = layout.spokenText(of: top.verse), let first = layout.bands(of: top.verse).first
                else { continue }
                let e = UIAccessibilityElement(accessibilityContainer: self)
                let ref = VerseRef(book: layout.content.book, chapter: layout.content.chapter, verse: UInt8(top.verse))
                e.accessibilityLabel = "\(top.verse). \(text)"
                e.accessibilityTraits = .staticText
                e.accessibilityIdentifier = rangeLabel(from: ref, to: ref)
                e.accessibilityFrameInContainerSpace = layout.bands(of: top.verse).dropFirst().reduce(first) { $0.union($1) }
                elements.append(e)
            }
            accessibilityCache = elements.sorted {
                $0.accessibilityFrameInContainerSpace.minY < $1.accessibilityFrameInContainerSpace.minY
            }
            return accessibilityCache
        }
        set {}
    }
}
