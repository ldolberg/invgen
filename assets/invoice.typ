// invgen invoice template.
// All data comes from /invoice.json (pre-formatted strings), never from markup.

#let d = json("/invoice.json")

// Palette
#let ink = rgb("#111827")
#let body = rgb("#374151")
#let muted = rgb("#6b7280")
#let faint = rgb("#9ca3af")
#let hair = rgb("#e5e7eb")
#let accent = rgb(d.accent)

#let status-colors = (
  pending: (fg: rgb("#b45309"), bg: rgb("#fef3c7"), dot: rgb("#f59e0b")),
  paid: (fg: rgb("#047857"), bg: rgb("#d1fae5"), dot: rgb("#10b981")),
  overdue: (fg: rgb("#b91c1c"), bg: rgb("#fee2e2"), dot: rgb("#ef4444")),
  draft: (fg: rgb("#4b5563"), bg: rgb("#f3f4f6"), dot: rgb("#9ca3af")),
)

#set document(title: "Invoice " + d.number, author: d.from.name)
#set page(
  paper: "a4",
  margin: (x: 20mm, top: 20mm, bottom: 22mm),
  footer: context {
    let total = counter(page).final().first()
    if total > 1 {
      set text(size: 7pt, fill: faint)
      grid(
        columns: (1fr, auto),
        d.number,
        [Page #counter(page).display() of #total],
      )
    }
  },
)
#set text(font: "Inter", size: 8.75pt, fill: body)
#let side = 66mm // width of the right-hand column, shared by both rows
#set par(leading: 0.55em, spacing: 0.55em)

// Helpers
#let label(t, fill: faint) = text(
  size: 6.5pt,
  weight: "semibold",
  tracking: 0.12em,
  fill: fill,
  upper(t),
)
#let lines(xs) = xs.map(x => [#x]).join(linebreak())
#let section(title, content, fill: faint) = block(width: 100%)[
  #label(title, fill: fill)
  #v(5pt)
  #content
]
#let party(p) = {
  text(weight: "semibold", fill: ink, p.name)
  if p.address.len() > 0 {
    linebreak()
    lines(p.address)
  }
}

#let pill(t, fg, bg, dot: none) = box(
  fill: bg,
  radius: 99pt,
  inset: (x: 7pt, y: 3.2pt),
  baseline: 2.6pt,
  {
    if dot != none {
      box(circle(radius: 2.2pt, fill: dot), baseline: -0.6pt)
      h(4pt)
    }
    text(size: 6.5pt, weight: "semibold", tracking: 0.08em, fill: fg, upper(t))
  },
)

// ---------------------------------------------------------------- Header
#grid(
  columns: (1fr, auto),
  align: (left + horizon, right + horizon),
  text(size: 20pt, weight: "semibold", fill: ink, tracking: -0.02em, d.from.name),
  box(
    stroke: 0.6pt + accent,
    radius: 99pt,
    inset: (x: 12pt, y: 5.5pt),
    text(size: 7.5pt, weight: "semibold", tracking: 0.32em, fill: accent, "INVOICE"),
  ),
)

#v(30pt)

// ---------------------------------------------------- Bill To + summary
#let sc = status-colors.at(d.status, default: status-colors.draft)
#let meta-row(k, v) = (text(fill: muted, k), v)
#let meta = (
  meta-row("Invoice", text(weight: "medium", fill: ink, "#" + d.number)),
  meta-row("Status", pill(d.status, sc.fg, sc.bg, dot: sc.dot)),
  meta-row("Invoice date", text(fill: ink, d.date)),
)
#if d.due != none { meta.push(meta-row("Due date", text(fill: ink, d.due))) }

#grid(
  columns: (1fr, side),
  column-gutter: 10mm,
  section("Bill to", {
    party(d.to)
    if d.to.tax.len() > 0 {
      v(6pt)
      text(fill: muted, lines(d.to.tax))
    }
    let contact = (d.to.email, d.to.phone).filter(x => x != none)
    if contact.len() > 0 {
      v(6pt)
      text(fill: muted, lines(contact))
    }
  }),
  {
    v(-4pt)
    grid(
      columns: (1fr, auto),
      rows: 15pt,
      align: (left + horizon, right + horizon),
      ..meta.flatten(),
    )
    v(9pt)
    line(length: 100%, stroke: 0.5pt + hair)
    v(9pt)
    grid(
      columns: (1fr, auto),
      align: (left + horizon, right + horizon),
      label("Total amount"),
      text(size: 16pt, weight: "semibold", fill: accent, tracking: -0.01em, number-width: "tabular", d.total),
    )
  },
)

#v(26pt)

// ------------------------------------------- From / Tax / Payment row
#let cols = ()
#cols.push(section("Bill from", {
  party(d.from)
}))
#if d.from.tax.len() > 0 {
  cols.push(section("Tax details", text(fill: muted, lines(d.from.tax))))
}
#if d.payment.holder != none or d.payment.iban != none or d.payment.bic != none or d.payment.notes.len() > 0 {
  cols.push(section("Payment method", {
    let p = d.payment
    if p.holder != none { text(weight: "medium", fill: ink, p.holder) }
    let kv = (("IBAN", p.iban), ("BIC", p.bic)).filter(x => x.at(1) != none)
    if kv.len() > 0 {
      if p.holder != none { v(4pt) }
      grid(
        columns: (auto, 1fr),
        column-gutter: 8pt,
        row-gutter: 0.55em,
        ..kv.map(((k, v)) => (text(fill: muted, k), box(text(fill: ink, number-width: "tabular", v)))).flatten(),
      )
    }
    if d.payment.notes.len() > 0 {
      v(4pt)
      text(fill: muted, lines(d.payment.notes))
    }
  }))
}
#grid(
  columns: if cols.len() == 3 { (1fr, 1fr, side) } else { (1fr, side) },
  column-gutter: 10mm,
  ..cols,
)

#v(30pt)

// ---------------------------------------------------------------- Items
#let th(t, al: left) = align(al, label(t, fill: muted))
#let num(t, fill: body, weight: "regular") = text(fill: fill, weight: weight, number-width: "tabular", t)

#table(
  columns: (1fr, 26mm, 26mm, 32mm),
  align: (left + top, right + top, right + top, right + top),
  inset: (x, y) => (left: if x == 0 { 0pt } else { 6pt }, right: 0pt, y: 8pt),
  stroke: (x, y) => if y == 0 { (bottom: 0.7pt + ink) } else { (bottom: 0.5pt + hair) },
  table.header(
    th("Description"),
    th("Qty (unit)", al: right),
    th("Rate", al: right),
    th("Total", al: right),
  ),
  ..d.items.map(it => (
    {
      text(weight: "medium", fill: ink, it.description)
      if it.detail != none {
        linebreak()
        text(size: 7.75pt, fill: muted, it.detail)
      }
    },
    {
      num(it.qty, fill: ink)
      if it.unit != "" { text(fill: muted, " " + it.unit) }
    },
    num(it.rate),
    num(it.total, fill: ink, weight: "medium"),
  )).flatten(),
)

// --------------------------------------------------------------- Totals
#v(4pt)
#align(right, block(width: side, breakable: false, {
  set par(spacing: 0pt)
  grid(
    columns: (1fr, auto),
    inset: (x: 0pt, y: 5pt),
    align: (left, right),
    text(fill: muted, "Subtotal"), num(d.subtotal),
    text(fill: muted, d.tax_label + " (" + d.tax_rate + ")"), num(d.tax),
  )
  v(4pt)
  block(
    width: 100%,
    inset: (x: 10pt, y: 10pt),
    radius: 4pt,
    fill: accent.lighten(94%),
    grid(
      columns: (1fr, auto),
      align: (left + horizon, right + horizon),
      text(weight: "semibold", fill: ink, "Total due"),
      text(size: 12pt, weight: "semibold", fill: accent, d.total),
    ),
  )
}))

// ---------------------------------------------------------------- Notes
#if d.notes.len() > 0 {
  v(22pt)
  block(width: 100%, breakable: true, {
    label("Notes")
    v(5pt)
    set par(spacing: 0.8em)
    for n in d.notes { par(text(fill: muted, n)) }
  })
}

// --------------------------------------------- Footer (end of last page)
#v(1fr)
#block(breakable: false, width: 100%, {
  v(28pt)
  line(length: 100%, stroke: 0.5pt + hair)
  v(12pt)
  grid(
    columns: (1fr, auto),
    align: (left + bottom, right + bottom),
    {
      let contact = (d.from.email, d.from.phone).filter(x => x != none)
      if contact.len() > 0 {
        label("Contact")
        v(5pt)
        text(fill: body, lines(contact))
      }
    },
    if d.signature != none {
      box(inset: (right: 6pt), {
        text(font: "Mrs Saint Delafield", size: 30pt, fill: ink, d.signature)
      })
    },
  )
})
