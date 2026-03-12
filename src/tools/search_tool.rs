use hex;
use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, schemars, tool};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{INDEXED, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value};
use tantivy::{Index, TantivyDocument, Term, doc};
use tracing;

// Search parameters: directory path and search keyword
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "Path to the directory to search")]
    pub directory: String,
    #[schemars(description = "Keyword to search for")]
    pub keyword: String,
}

// File content parameters: file path
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FileContentParams {
    #[schemars(description = "Path to the file to read")]
    pub file_path: String,
}

// Main tool struct
#[derive(Debug, Clone)]
pub struct SearchTool;

#[tool(tool_box)]
impl SearchTool {
    pub fn new() -> Self {
        Self {}
    }

    /// Get the cache directory for a specific search path
    fn get_index_path(&self, directory: &str) -> Result<PathBuf, String> {
        let abs_path = fs::canonicalize(directory)
            .map_err(|e| format!("Failed to canonicalize path '{}': {}", directory, e))?;

        let mut hasher = Sha256::new();
        hasher.update(abs_path.to_string_lossy().as_bytes());
        let hash = hex::encode(hasher.finalize());

        let mut cache_dir =
            dirs::cache_dir().ok_or_else(|| "Could not determine cache directory".to_string())?;
        cache_dir.push("file-search-mcp");
        cache_dir.push(hash);

        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)
                .map_err(|e| format!("Failed to create cache directory: {}", e))?;
        }

        Ok(cache_dir)
    }

    /// Define the Tantivy schema
    fn get_schema(&self) -> Schema {
        let mut schema_builder = Schema::builder();
        // Use STRING for path to make it easy to delete/lookup (not tokenized)
        schema_builder.add_text_field("path", STRING | STORED);

        // Content field for full-text search
        let text_indexing = TextFieldIndexing::default().set_tokenizer("default");
        let text_options = TextOptions::default()
            .set_indexing_options(text_indexing)
            .set_stored();
        schema_builder.add_text_field("content", text_options);

        // Modified time for incremental indexing
        schema_builder.add_i64_field("modified", INDEXED | STORED);

        schema_builder.build()
    }

    /// Read and return the content of a specified file
    #[tool(description = "Read the content of a file from the specified path")]
    async fn read_file_content(
        &self,
        #[tool(aggr)] params: FileContentParams,
    ) -> Result<String, String> {
        // Validate file path
        let file_path = Path::new(&params.file_path);

        // Check if the path exists
        if !file_path.exists() {
            return Err(format!(
                "The specified path '{}' does not exist",
                params.file_path
            ));
        }

        // Check if the path is a file
        if !file_path.is_file() {
            return Err(format!(
                "The specified path '{}' is not a file",
                params.file_path
            ));
        }

        // Try to read the file content
        match fs::read_to_string(file_path) {
            Ok(content) => {
                if content.is_empty() {
                    Ok("File is empty.".to_string())
                } else {
                    Ok(content)
                }
            }
            Err(e) => {
                // Handle binary files or read errors
                tracing::error!("Error reading file '{}': {}", file_path.display(), e);

                // Try to read as binary and check if it's a binary file
                match fs::read(file_path) {
                    Ok(bytes) => {
                        // Check if it seems to be a binary file
                        if bytes.iter().any(|&b| b == 0)
                            || bytes
                                .iter()
                                .filter(|&&b| b < 32 && b != 9 && b != 10 && b != 13)
                                .count()
                                > bytes.len() / 10
                        {
                            Err(format!(
                                "The file '{}' appears to be a binary file and cannot be displayed as text",
                                params.file_path
                            ))
                        } else {
                            Err(format!(
                                "The file '{}' could not be read as text: {}",
                                params.file_path, e
                            ))
                        }
                    }
                    Err(read_err) => Err(format!(
                        "Error reading file '{}': {}",
                        params.file_path, read_err
                    )),
                }
            }
        }
    }

    /// Perform full-text search for keywords on text files (such as .txt, .md, etc.) in the specified directory
    #[tool(description = "Search for keywords in text files within the specified directory")]
    async fn search(&self, #[tool(aggr)] params: SearchParams) -> Result<String, String> {
        let start_time = std::time::Instant::now();

        // 1. Prepare schema and index path
        let schema = self.get_schema();
        let path_field = schema.get_field("path").map_err(|e| e.to_string())?;
        let content_field = schema.get_field("content").map_err(|e| e.to_string())?;
        let modified_field = schema.get_field("modified").map_err(|e| e.to_string())?;

        let index_path = self.get_index_path(&params.directory)?;
        tracing::debug!("Using index at: {}", index_path.display());

        // 2. Open or create index
        let index = if index_path.join("meta.json").exists() {
            Index::open_in_dir(&index_path).map_err(|e| format!("Failed to open index: {}", e))?
        } else {
            Index::create_in_dir(&index_path, schema.clone())
                .map_err(|e| format!("Failed to create index: {}", e))?
        };

        // 3. Load existing documents' modification times to support incremental indexing
        let mut existing_files: HashMap<String, i64> = HashMap::new();
        {
            let reader = index.reader().map_err(|e| e.to_string())?;
            let searcher = reader.searcher();
            let segment_readers = searcher.segment_readers();
            for segment_reader in segment_readers {
                let store_reader = segment_reader
                    .get_store_reader(100)
                    .map_err(|e| e.to_string())?;
                for doc_id in 0..segment_reader.max_doc() {
                    if !segment_reader.is_deleted(doc_id) {
                        let doc: TantivyDocument =
                            store_reader.get(doc_id).map_err(|e| e.to_string())?;
                        if let (Some(path), Some(modified)) = (
                            doc.get_first(path_field).and_then(|v| v.as_str()),
                            doc.get_first(modified_field).and_then(|v| v.as_i64()),
                        ) {
                            existing_files.insert(path.to_string(), modified);
                        }
                    }
                }
            }
        }

        // 4. Create index writer
        let mut index_writer = index
            .writer(50_000_000)
            .map_err(|e| format!("Index writer error: {}", e))?;

        // Count the number of files added to the index
        let mut indexed_files_count = 0;
        let mut skipped_files_count = 0;
        let mut deleted_files_count = 0;

        // 4. Read text files in the specified directory and add them to the index
        let dir_path = Path::new(&params.directory);
        if !dir_path.is_dir() {
            return Err(format!(
                "The specified path '{}' is not a directory",
                params.directory
            ));
        }

        // Directories to always skip (common heavy/internal dirs)
        let skip_dirs = [
            ".git", "target", "node_modules", ".vscode", "build", "dist",
            ".venv", "venv", "env", "__pycache__", ".tox",
            ".next", ".nuxt", ".svelte-kit", ".turbo",
        ];

        // Blacklist of extensions likely to be binary files
        // Skip extensions that are clearly binary files
        let binary_extensions = [
            "exe", "dll", "so", "dylib", "bin", "obj", "o", "a", "lib", "png", "jpg", "jpeg",
            "gif", "bmp", "tiff", "webp", "ico", "mp3", "mp4", "wav", "ogg", "flac", "avi", "mov",
            "mkv", "zip", "gz", "tar", "7z", "rar", "jar", "war", "pdf", "doc", "docx", "xls",
            "xlsx", "ppt", "pptx", "db", "sqlite", "mdb", "iso", "dmg", "class",
        ];

        // Track files found in current scan to identify deleted files later
        let mut current_scan_files = HashMap::new();

        // Recursively process directory
        fn scan_directory(
            dir: &Path,
            skip_dirs: &[&str],
            binary_extensions: &[&str],
            files: &mut HashMap<PathBuf, i64>,
        ) -> Result<(), String> {
            for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if skip_dirs.contains(&name) {
                            continue;
                        }
                    }
                    scan_directory(&path, skip_dirs, binary_extensions, files)?;
                } else if path.is_file() {
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        if binary_extensions.contains(&ext_str.as_str()) {
                            continue;
                        }
                    }
                    let mtime = fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
                        .unwrap_or(0);
                    files.insert(path, mtime);
                }
            }
            Ok(())
        }

        scan_directory(
            dir_path,
            &skip_dirs,
            &binary_extensions,
            &mut current_scan_files,
        )?;
        let found_files_count = current_scan_files.len();

        // Incremental Update Logic
        for (path, mtime) in current_scan_files {
            let path_str = path.to_string_lossy().to_string();
            let needs_update = match existing_files.get(&path_str) {
                Some(&old_mtime) => old_mtime != mtime,
                None => true,
            };

            if needs_update {
                if is_text_file(&path) {
                    match fs::read_to_string(&path) {
                        Ok(content) => {
                            if !content.trim().is_empty() {
                                // Delete old version if it exists
                                index_writer
                                    .delete_term(Term::from_field_text(path_field, &path_str));

                                index_writer
                                    .add_document(doc!(
                                        path_field => path_str,
                                        content_field => content,
                                        modified_field => mtime,
                                    ))
                                    .map_err(|e| e.to_string())?;
                                indexed_files_count += 1;
                                tracing::debug!("Indexed/Updated: {}", path.display());
                            } else {
                                skipped_files_count += 1;
                            }
                        }
                        Err(_) => skipped_files_count += 1,
                    }
                } else {
                    skipped_files_count += 1;
                }
            }
        }

        // Identify and remove deleted files
        for path_str in existing_files.keys() {
            if !Path::new(path_str).exists() {
                index_writer.delete_term(Term::from_field_text(path_field, path_str));
                deleted_files_count += 1;
                tracing::debug!("Deleted from index: {}", path_str);
            }
        }

        fn is_text_file(path: &Path) -> bool {
            match fs::read(path) {
                Ok(bytes) if !bytes.is_empty() => {
                    let sample_size = std::cmp::min(bytes.len(), 8192);
                    let sample = &bytes[..sample_size];
                    if sample.iter().any(|&b| b == 0) {
                        return false;
                    }
                    let control_chars = sample
                        .iter()
                        .filter(|&&b| b < 32 && b != 9 && b != 10 && b != 13)
                        .count();
                    if (control_chars as f32 / sample_size as f32) > 0.3 {
                        return false;
                    }
                    std::str::from_utf8(sample).is_ok()
                        || (sample.iter().filter(|&&b| b <= 127).count() as f32
                            / sample_size as f32)
                            > 0.8
                }
                _ => false,
            }
        }

        let indexing_duration = start_time.elapsed();
        tracing::debug!(
            "Indexing complete in {:?}: Found={}, Indexed/Updated={}, Deleted={}, Skipped={}",
            indexing_duration,
            found_files_count,
            indexed_files_count,
            deleted_files_count,
            skipped_files_count
        );

        // 5. Commit changes
        index_writer
            .commit()
            .map_err(|e| format!("Commit error: {}", e))?;

        // 6. Search
        let search_start = std::time::Instant::now();
        let reader = index.reader().map_err(|e| e.to_string())?;
        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&index, vec![content_field]);

        if params.keyword.trim().is_empty() {
            return Err("Search keyword is empty.".into());
        }

        let query = query_parser
            .parse_query(&params.keyword)
            .map_err(|e| format!("Query parse error: {}", e))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(10))
            .map_err(|e| format!("Search error: {}", e))?;

        let mut result_str = String::new();
        for (score, doc_address) in &top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(*doc_address).map_err(|e| e.to_string())?;
            let path_value = retrieved_doc
                .get_first(path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown path");
            result_str.push_str(&format!("Hit: {} (Score: {:.2})\n", path_value, score));
        }

        let total_duration = start_time.elapsed();
        tracing::debug!("Search completed in {:?}", search_start.elapsed());


        if result_str.is_empty() {
            Ok(format!(
                "No results for '{}' in '{}'. (took {:?})",
                params.keyword, params.directory, total_duration
            ))
        } else {
            Ok(format!(
                "Search results ({} hits, took {:?} total):\n{}",
                top_docs.len(),
                total_duration,
                result_str
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_skip_dirs_logic() {
        let skip_dirs = [
            ".git", "target", "node_modules", ".vscode", "build", "dist",
            ".venv", "venv", "env", "__pycache__", ".tox",
            ".next", ".nuxt", ".svelte-kit", ".turbo",
        ];
        let test_cases = [
            (".git", true),
            ("target", true),
            ("src", false),
            ("node_modules", true),
            (".venv", true),
            ("__pycache__", true),
            ("my_folder", false),
        ];

        for (dir_name, expected_skip) in test_cases {
            let path = Path::new(dir_name);
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let should_skip = skip_dirs.iter().any(|&d| d == name);
            assert_eq!(
                should_skip, expected_skip,
                "Failed for directory: {}",
                dir_name
            );
        }
    }

    #[test]
    fn test_index_path_is_consistent() {
        let tool = SearchTool::new();
        let path1 = tool.get_index_path(".").unwrap();
        let path2 = tool.get_index_path(".").unwrap();
        assert_eq!(
            path1, path2,
            "Index path should be consistent for the same directory"
        );

        // Use a definitely different path
        let path3 = tool.get_index_path("/tmp").unwrap();
        assert_ne!(
            path1, path3,
            "Different directories should have different index paths"
        );
    }
}

#[tool(tool_box)]
impl ServerHandler for SearchTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_prompts()
                .enable_resources()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "This server provides two tools: 1) Search for keywords in text files within a directory, 2) Read and display the content of a specific file."
                    .into(),
            ),
        }
    }
}
