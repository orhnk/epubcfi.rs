use serde_json::Value;
use std::fs;
use std::io::{self, Write};
use std::process::Command;

struct SearchResult {
    start_index: usize,
    end_index: usize,
    start_paragraph: String,
    end_paragraph: String,
    start_cfi: String,
    end_cfi: String,
    start_offset_in_paragraph: usize,
    end_offset_in_paragraph: usize,
}

// Normalize spaces by removing them
fn normalize_text(input: &str) -> String {
    input.chars().filter(|c| !c.is_whitespace()).collect()
}

fn find_paragraph_range(
    char_start: usize,
    char_end: usize,
    paragraph_indexes: &[(usize, usize)],
    paragraphs: &[String],
    cfis: &[String],
) -> Option<(String, String, String, String, usize, usize)> {
    let mut start_paragraph = None;
    let mut end_paragraph = None;
    let mut start_cfi = None;
    let mut end_cfi = None;
    let mut start_offset_in_paragraph = None;
    let mut end_offset_in_paragraph = None;

    for (i, &(start, end)) in paragraph_indexes.iter().enumerate() {
        if char_start >= start && char_start <= end {
            start_paragraph = Some(paragraphs[i].clone());
            start_cfi = Some(cfis[i].clone());
            start_offset_in_paragraph = Some(char_start - start);
        }
        if char_end >= start && char_end <= end {
            end_paragraph = Some(paragraphs[i].clone());
            end_cfi = Some(cfis[i].clone());
            end_offset_in_paragraph = Some(char_end - start);
        }
        if start_paragraph.is_some() && end_paragraph.is_some() {
            break;
        }
    }

    match (
        start_paragraph,
        end_paragraph,
        start_cfi,
        end_cfi,
        start_offset_in_paragraph,
        end_offset_in_paragraph,
    ) {
        (
            Some(start_paragraph),
            Some(end_paragraph),
            Some(start_cfi),
            Some(end_cfi),
            Some(start_offset_in_paragraph),
            Some(end_offset_in_paragraph),
        ) => Some((
            start_paragraph,
            end_paragraph,
            start_cfi,
            end_cfi,
            start_offset_in_paragraph,
            end_offset_in_paragraph,
        )),
        _ => None,
    }
}

fn search_query(
    query: &str,
    normalized_text: &str,
    paragraph_indexes: &[(usize, usize)],
    paragraphs: &[String],
    cfis: &[String],
) -> Result<SearchResult, &'static str> {
    let normalized_query = normalize_text(query);

    if let Some(start_index) = normalized_text.find(&normalized_query) {
        let end_index = start_index + normalized_query.len();

        if let Some((
            start_paragraph,
            end_paragraph,
            start_cfi,
            end_cfi,
            start_offset_in_paragraph,
            end_offset_in_paragraph,
        )) = find_paragraph_range(start_index, end_index, paragraph_indexes, paragraphs, cfis)
        {
            return Ok(SearchResult {
                start_index,
                end_index,
                start_paragraph,
                end_paragraph,
                start_cfi,
                end_cfi,
                start_offset_in_paragraph,
                end_offset_in_paragraph,
            });
        }
    }

    Err("Query not found")
}

fn generate_database(epub_path: &str, output_path: &str) -> io::Result<()> {
    // Run the epub-cfi-generator command
    let output = Command::new("node")
        .arg("./node_modules/.bin/epub-cfi-generator")
        .arg(epub_path)
        .arg(output_path)
        .output()?;

    if !output.status.success() {
        io::stderr().write_all(&output.stderr)?;
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to generate database",
        ));
    }

    Ok(())
}

fn format_epub_cfi(
    start_cfi: &str,
    end_cfi: &str,
    start_offset: usize,
    end_offset: usize,
) -> String {
    // Split the CFIs into path and offset
    let (start_path, _) = start_cfi.split_once(':').unwrap_or((start_cfi, "0"));
    let (end_path, _) = end_cfi.split_once(':').unwrap_or((end_cfi, "0"));

    // Remove trailing numbers in paths
    let sanitized_start_path = start_path.rsplit_once('/').map_or("", |(path, _)| path);
    let sanitized_end_path = end_path.rsplit_once('/').map_or("", |(path, _)| path);

    // Find the common part of the paths
    // Skip the first element since it's an empty string due to the leading slash
    let start_path_parts: Vec<&str> = sanitized_start_path.split('/').skip(1).collect();
    let end_path_parts: Vec<&str> = sanitized_end_path.split('/').skip(1).collect();

    let mut common_part = String::new();
    let mut i = 0;

    while i < start_path_parts.len()
        && i < end_path_parts.len()
        && start_path_parts[i] == end_path_parts[i]
    {
        common_part.push('/');
        common_part.push_str(start_path_parts[i]);
        i += 1;
    }

    // Compute the dissimilar parts
    let dissimilar_start = &sanitized_start_path[common_part.len()..];
    let dissimilar_end = &sanitized_end_path[common_part.len()..];

    // Format the final output
    let start_entity = format!("{dissimilar_start}/1:{start_offset}");
    let end_entity = format!("{dissimilar_end}/1:{end_offset}");

    format!("epubcfi({},{},{})", common_part, start_entity, end_entity)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let epub_path = "/home/nixos/src/scripts/last3/Mesnevi-i Nuriye - Bediuzzaman Said Nursi.epub";
    let query = "n Resul-i Ekrem Aleyhissalâtü Vesselâm’ına ümmet eylemiş. 
ONUNCU NOTA: Bil ey gafil, müşevveş Said! Cenab-ı Hakk’ın nur-u marifetine yetişmek ve bakmak ve âyât ve şahidlerin âyinelerinde cilvelerini görmek";

    let data_path = "data.json";

    if !fs::metadata(data_path).is_ok() {
        generate_database(epub_path, data_path)?;
    }

    // Read and parse the JSON data
    let data = fs::read_to_string(data_path)?;
    let json: Value = serde_json::from_str(&data)?;

    // Extract paragraphs and calculate character indexes
    let mut paragraphs = Vec::new();
    let mut paragraph_indexes = Vec::new();
    let mut cfis = Vec::new();
    let mut concat_text = String::new();

    if let Some(contents) = json.as_array() {
        let mut char_index = 0;

        for entry in contents {
            if let Some(content) = entry["content"].as_array() {
                for paragraph in content {
                    if let Some(node) = paragraph["node"].as_str() {
                        if let Some(cfi) = paragraph["cfi"].as_str() {
                            let normalized_node = normalize_text(node);
                            let start_index = char_index;
                            let end_index = char_index + normalized_node.len();

                            paragraphs.push(node.to_string());
                            paragraph_indexes.push((start_index, end_index));
                            cfis.push(cfi.to_string());
                            concat_text.push_str(&normalized_node);

                            char_index = end_index;
                        }
                    }
                }
            }
        }
    }

    let normalized_text = normalize_text(&concat_text);

    match search_query(
        query,
        &normalized_text,
        &paragraph_indexes,
        &paragraphs,
        &cfis,
    ) {
        Ok(result) => {
            println!("Query found:");
            println!("Start index: {}", result.start_index);
            println!("End index: {}", result.end_index);
            println!("Start paragraph: {}", result.start_paragraph);
            println!("End paragraph: {}", result.end_paragraph);
            println!("Start CFI: {}", result.start_cfi);
            println!("End CFI: {}", result.end_cfi);
            println!(
                "Start offset in paragraph: {}",
                result.start_offset_in_paragraph
            );
            println!(
                "End offset in paragraph: {}",
                result.end_offset_in_paragraph
            );

            let formatted_cfi = format_epub_cfi(
                &result.start_cfi,
                &result.end_cfi,
                result.start_offset_in_paragraph,
                result.end_offset_in_paragraph,
            );

            println!("Formatted CFI: {}", formatted_cfi);
            println!("EXPECTED:      epubcfi(/6/42!/4,/132/4/1:261,/134/5:135)");
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    Ok(())
}
