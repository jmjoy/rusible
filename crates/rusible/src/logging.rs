use ansi_term::Color;
use std::fmt::{self, Write as _};
use tracing::Level;
use tracing_forest::{
    ForestLayer, Tag,
    printer::{Formatter, PrettyPrinter},
    tree::{Event, Span, Tree},
};
use tracing_subscriber::{
    EnvFilter, Registry, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

pub fn init_forest_logging(default_filter: &str) {
    Registry::default()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter)))
        .with(ForestLayer::from(
            PrettyPrinter::new().formatter(CompactFormatter),
        ))
        .init();
}

#[derive(Clone, Copy, Debug, Default)]
struct CompactFormatter;

impl Formatter for CompactFormatter {
    type Error = fmt::Error;

    fn fmt(&self, tree: &Tree) -> Result<String, Self::Error> {
        let mut writer = String::with_capacity(256);
        format_tree(tree, None, &mut Vec::new(), &mut writer)?;
        Ok(writer)
    }
}

fn format_tree(
    tree: &Tree, duration_root: Option<f64>, indent: &mut Vec<Indent>, writer: &mut String,
) -> fmt::Result {
    match tree {
        Tree::Event(event) => {
            format_shared(
                event.level(),
                event.timestamp().format("%H:%M:%S%.3f"),
                writer,
            )?;
            format_indent(indent, writer)?;
            format_event(event, writer)
        }
        Tree::Span(span) => {
            format_shared(
                span.level(),
                span.timestamp().format("%H:%M:%S%.3f"),
                writer,
            )?;
            format_indent(indent, writer)?;
            format_span(span, duration_root, indent, writer)
        }
    }
}

fn format_shared(level: Level, timestamp: impl fmt::Display, writer: &mut String) -> fmt::Result {
    write!(writer, "{timestamp} {:<17} ", paint_level(level))
}

fn format_indent(indent: &[Indent], writer: &mut String) -> fmt::Result {
    for indent in indent {
        writer.write_str(indent.repr())?;
    }
    Ok(())
}

fn format_event(event: &Event, writer: &mut String) -> fmt::Result {
    let tag = event.tag().unwrap_or_else(|| Tag::from(event.level()));
    write!(writer, "{} [{}]: ", tag.icon(), tag)?;

    if let Some(message) = event.message() {
        writer.write_str(message)?;
    }

    for field in event.fields() {
        write!(
            writer,
            " | {}: {}",
            paint_field_key(field.key()),
            paint_field_value(field.key(), field.value())
        )?;
    }

    writeln!(writer)
}

fn format_span(
    span: &Span, duration_root: Option<f64>, indent: &mut Vec<Indent>, writer: &mut String,
) -> fmt::Result {
    let total_duration = span.total_duration().as_nanos() as f64;
    let inner_duration = span.inner_duration().as_nanos() as f64;
    let root_duration = duration_root.unwrap_or(total_duration);
    let percent_total_of_root_duration = 100.0 * total_duration / root_duration;

    write!(
        writer,
        "{} [ {} | ",
        span.name(),
        DurationDisplay(total_duration)
    )?;

    if inner_duration > 0.0 {
        let base_duration = span.base_duration().as_nanos() as f64;
        let percent_base_of_root_duration = 100.0 * base_duration / root_duration;
        write!(writer, "{percent_base_of_root_duration:.2}% / ")?;
    }

    write!(writer, "{percent_total_of_root_duration:.2}% ]")?;

    for (index, field) in span.fields().iter().enumerate() {
        write!(
            writer,
            "{} {}: {}",
            if index == 0 { "" } else { " |" },
            paint_field_key(field.key()),
            paint_field_value(field.key(), field.value())
        )?;
    }
    writeln!(writer)?;

    let nodes = span.nodes().iter().collect::<Vec<_>>();
    if let Some((last, remaining)) = nodes.split_last() {
        match indent.last_mut() {
            Some(edge @ Indent::Turn) => *edge = Indent::Null,
            Some(edge @ Indent::Fork) => *edge = Indent::Line,
            _ => {}
        }

        indent.push(Indent::Fork);

        for tree in remaining {
            if let Some(edge) = indent.last_mut() {
                *edge = Indent::Fork;
            }
            format_tree(tree, Some(root_duration), indent, writer)?;
        }

        if let Some(edge) = indent.last_mut() {
            *edge = Indent::Turn;
        }
        format_tree(last, Some(root_duration), indent, writer)?;

        indent.pop();
    }

    Ok(())
}

fn paint_level(level: Level) -> ansi_term::ANSIGenericString<'static, str> {
    let color = match level {
        Level::TRACE => Color::Purple,
        Level::DEBUG => Color::Blue,
        Level::INFO => Color::Green,
        Level::WARN => Color::RGB(252, 234, 160),
        Level::ERROR => Color::Red,
    };

    color.bold().paint(level.as_str().to_string())
}

fn paint_field_key(key: &str) -> ansi_term::ANSIGenericString<'static, str> {
    Color::White.dimmed().paint(key.to_string())
}

fn paint_field_value(key: &str, value: &str) -> ansi_term::ANSIGenericString<'static, str> {
    if key.eq_ignore_ascii_case("status") {
        return paint_status(value);
    }

    Color::White.paint(value.to_string())
}

fn paint_status(value: &str) -> ansi_term::ANSIGenericString<'static, str> {
    let normalized = value.trim_matches('"').to_ascii_lowercase();
    let style = match normalized.as_str() {
        "ok" => Color::Green.normal(),
        "changed" => Color::Yellow.bold(),
        "skipped" => Color::Blue.normal(),
        "failed" | "unreachable" => Color::Red.bold(),
        _ => Color::White.normal(),
    };

    style.paint(value.to_string())
}

#[derive(Clone, Copy)]
enum Indent {
    Null,
    Line,
    Fork,
    Turn,
}

impl Indent {
    fn repr(&self) -> &'static str {
        match self {
            Self::Null => "   ",
            Self::Line => "│  ",
            Self::Fork => "┝━ ",
            Self::Turn => "┕━ ",
        }
    }
}

struct DurationDisplay(f64);

impl fmt::Display for DurationDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut value = self.0;
        for unit in ["ns", "µs", "ms", "s"] {
            if value < 10.0 {
                return write!(f, "{value:.2}{unit}");
            }
            if value < 100.0 {
                return write!(f, "{value:.1}{unit}");
            }
            if value < 1000.0 {
                return write!(f, "{value:.0}{unit}");
            }
            value /= 1000.0;
        }

        write!(f, "{:.0}s", value * 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::info;

    #[tokio::test]
    async fn compact_formatter_omits_uuid_and_keeps_short_timestamp() {
        let logs = tracing_forest::capture()
            .build()
            .on(async {
                info!(host = "node-1", status = "changed", "hello");
            })
            .await;

        let rendered = CompactFormatter.fmt(&logs[0]).unwrap();
        assert!(!rendered.contains("00000000-0000-0000-0000"));
        assert!(rendered.contains("hello"));
        assert!(rendered.contains("node-1"));
        assert!(rendered.contains("changed"));
        assert!(rendered.chars().take(12).filter(|c| *c == ':').count() >= 2);
    }
}
