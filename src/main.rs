use std::any::Any;
use std::env;
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use tree_sitter::Point;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SemanticFile {
    #[serde(rename = "type")]
    item_type: String,
    name: String,
    location_span: LocationSpan,
    footer_span: CharSpan,
    parsing_errors_detected: bool,
    children: Vec<Node>,
    parsing_error: Option<()>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", untagged)]
enum Node {
    Container(Container),
    Terminal(Terminal),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Container {
    #[serde(rename = "type")]
    item_type: String,
    name: String,
    location_span: LocationSpan,
    header_span: CharSpan,
    footer_span: CharSpan,
    children: Vec<Node>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Terminal {
    #[serde(rename = "type")]
    item_type: String,
    name: String,
    location_span: LocationSpan,
    span: CharSpan,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ParsingError {
    location: LocationSpan,
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LocationSpan {
    start: [i32; 2],
    end: [i32; 2],
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", transparent)]
struct CharSpan {
    span: [i32; 2],
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut console = std::fs::File::create("output.txt").unwrap();
    writeln!(console, "{:?}", args);

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    save_file(args[2].as_str(), "hello");

    let mut input_path = String::new();
    let mut output_path = String::new();
    loop {
        input_path.clear();
        stdin.lock().read_line(&mut input_path);
        if input_path.contains("end") {
            writeln!(console, "Done...");
            break;
        }

        stdin.lock().read_line(&mut output_path);
        output_path.clear();
        stdin.lock().read_line(&mut output_path);
        input_path = input_path.split_whitespace().next().unwrap().to_string();
        output_path = output_path.split_whitespace().next().unwrap().to_string();
        writeln!(console, ":: {} -> {}", input_path, output_path);

        let file_contents = read_file(&input_path);
        if let Ok(file_contents) = file_contents {
            let line_count = file_contents.lines().count();
            let last_pos = file_contents.lines().last().unwrap().len();
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(tree_sitter_rust::language());
            let mut tree = parser.parse(&file_contents, None).unwrap();

            let mut file_node = SemanticFile {
                item_type: "file".to_string(),
                name: input_path.to_string(),
                location_span: LocationSpan {
                    start: [1, 0],
                    end: [line_count as i32, last_pos as i32],
                },
                footer_span: CharSpan { span: [0, -1] },
                parsing_errors_detected: false,
                children: vec![],
                parsing_error: None,
            };

            let mut node = tree.root_node();

            fn walk_tree(
                node: &mut tree_sitter::Node,
                file_contents: &str,
                console: &mut File,
            ) -> anyhow::Result<Node> {
                let kind = node.kind();
                let mut contents: String = node
                    .utf8_text(file_contents.as_bytes())
                    .unwrap_or("")
                    .to_string();
                let name = if kind.contains("identifier") || kind.contains("item") {
                    contents = contents
                        .replace("{", " ")
                        .replace("}", " ")
                        .replace("(", " ")
                        .replace(")", " ")
                        .replace(":", " ")
                        .replace("#", " ")
                        .replace("[", " ")
                        .replace("]", " ")
                        .replace("fn", " ")
                        .replace("struct", " ")
                        .replace("enum", " ")
                        .replace("pub", " ");
                    let mut name_iter = contents.split_whitespace();
                    name_iter.next().ok_or(anyhow::anyhow!("Failed"))?
                } else {
                    kind
                };

                let child_count = node.named_child_count();

                if child_count == 0 {
                    Ok(Node::Terminal(Terminal {
                        item_type: node.kind().to_string(),
                        name: name.to_string(),
                        location_span: LocationSpan {
                            start: convert_point(node.start_position()),
                            end: convert_point(node.end_position()),
                        },
                        span: CharSpan {
                            span: [node.start_byte() as i32, node.end_byte() as i32],
                        },
                    }))
                } else {
                    let mut children = vec![];
                    for i in 0..child_count {
                        let mut child_node = node.named_child(i).unwrap();
                        let child = walk_tree(&mut child_node, file_contents, console);
                        if let Ok(child) = child {
                            children.push(child);
                        }
                    }

                    Ok(Node::Container(Container {
                        item_type: node.kind().to_string(),
                        name: name.to_string(),
                        location_span: LocationSpan {
                            start: convert_point(node.start_position()),
                            end: convert_point(node.end_position()),
                        },
                        header_span: CharSpan {
                            span: [node.start_byte() as i32, node.end_byte() as i32],
                        },
                        footer_span: CharSpan { span: [0, -1] },
                        children,
                    }))
                }
            }

            let children = walk_tree(&mut node, &file_contents, &mut console).unwrap();
            file_node.children = match children {
                Node::Container(c) => c.children,
                Node::Terminal(_) => unreachable!(),
            };
            let serialized = serde_json::to_string_pretty(&file_node).unwrap();
            save_file(&output_path, &serialized);
            writeln!(console, "{}", serialized);
            stdout.lock().write("OK\n".as_ref());
        } else {
            save_file(&output_path, "dum");
            stdout.lock().write("OK\n".as_ref());
        }
    }
}

fn read_file(path: &str) -> anyhow::Result<String> {
    let mut f = File::open(path)?;
    let mut result = String::new();
    f.read_to_string(&mut result);
    Ok(result)
}

fn save_file(path: &str, file: &str) {
    let mut f = File::create(path).unwrap();
    f.write(file.as_bytes());
}

fn convert_point(p: Point) -> [i32; 2] {
    [p.row as i32 + 1, p.column as i32]
}
