use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{error, info, warn};

/// Visual Analysis Report for captured terminal output
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VisualAnalysisReport {
    pub session_info: SessionInfo,
    pub ui_state: UIState,
    pub performance_metrics: PerformanceMetrics,
    pub visual_quality: VisualQuality,
    pub issues_found: Vec<Issue>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionInfo {
    pub duration_seconds: f32,
    pub total_lines: usize,
    pub file_size_bytes: usize,
    pub timestamp: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UIState {
    pub professional_ui_active: bool,
    pub demo_mode_activated: bool,
    pub audio_capture_working: bool,
    pub current_visualization_mode: Option<String>,
    pub theme_switches: usize,
    pub mode_switches: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceMetrics {
    pub fps_measurements: Vec<f32>,
    pub average_fps: f32,
    pub max_fps: f32,
    pub min_fps: f32,
    pub audio_packets_received: usize,
    pub compilation_warnings: usize,
    pub errors_found: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VisualQuality {
    pub color_variety_score: f32, // 0-1, based on color escape sequences found
    pub animation_smoothness: f32, // 0-1, based on frame consistency
    pub rendering_complexity: f32, // 0-1, based on character variety and patterns
    pub responsiveness_score: f32, // 0-1, based on audio response timing
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Issue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub description: String,
    pub line_number: Option<usize>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IssueSeverity {
    Critical, // Blocks functionality
    High,     // Significantly impacts UX
    Medium,   // Minor UX impact
    Low,      // Polish/optimization
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IssueCategory {
    Performance,
    VisualQuality,
    AudioPipeline,
    UserInterface,
    Compilation,
}

pub fn analyze_visual_output(file_path: &Path) -> Result<VisualAnalysisReport> {
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();

    info!("🔍 Analyzing visual output: {:?}", file_path);
    info!("📊 Total lines to analyze: {}", lines.len());

    let mut report = VisualAnalysisReport {
        session_info: analyze_session_info(&content, file_path)?,
        ui_state: analyze_ui_state(&lines),
        performance_metrics: analyze_performance(&lines),
        visual_quality: analyze_visual_quality(&lines),
        issues_found: Vec::new(),
        recommendations: Vec::new(),
    };

    // Find issues based on analysis
    report.issues_found = find_issues(&report);

    // Generate recommendations
    report.recommendations = generate_recommendations(&report);

    Ok(report)
}

fn analyze_session_info(content: &str, file_path: &Path) -> Result<SessionInfo> {
    let metadata = fs::metadata(file_path)?;
    let file_size = metadata.len() as usize;
    let lines = content.lines().count();

    // Try to extract duration from content
    let duration = extract_duration_from_content(content);

    Ok(SessionInfo {
        duration_seconds: duration,
        total_lines: lines,
        file_size_bytes: file_size,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

fn extract_duration_from_content(content: &str) -> f32 {
    // Look for timestamps or duration indicators
    let timestamp_re = Regex::new(r"2025-\d{2}-\d{2}T(\d{2}):(\d{2}):(\d{2})").unwrap();
    let mut timestamps = Vec::new();

    for cap in timestamp_re.captures_iter(content) {
        if let (Ok(h), Ok(m), Ok(s)) = (
            cap[1].parse::<u32>(),
            cap[2].parse::<u32>(),
            cap[3].parse::<u32>(),
        ) {
            let total_seconds = h * 3600 + m * 60 + s;
            timestamps.push(total_seconds);
        }
    }

    if timestamps.len() >= 2 {
        let duration = timestamps.last().unwrap() - timestamps.first().unwrap();
        duration as f32
    } else {
        // Estimate based on content size (rough heuristic)
        (content.len() / 10000) as f32
    }
}

fn analyze_ui_state(lines: &[&str]) -> UIState {
    let mut professional_ui = false;
    let mut demo_mode = false;
    let mut audio_capture = false;
    let mut current_mode = None;
    let mut theme_switches = 0;
    let mut mode_switches = 0;

    for line in lines {
        if line.contains("Professional UI mode") || line.contains("v1.0 interface") {
            professional_ui = true;
        }

        if line.contains("🎭 Activating demo mode") {
            demo_mode = true;
        }

        if line.contains("Audio capture started") || line.contains("🎵 Audio capture started") {
            audio_capture = true;
        }

        if line.contains("🔄 Switched visualization mode to:") {
            mode_switches += 1;
            // Extract mode name
            if let Some(mode_start) = line.find("mode to: ") {
                let mode_part = &line[mode_start + 9..];
                if let Some(mode_end) = mode_part.find(' ') {
                    current_mode = Some(mode_part[..mode_end].to_string());
                } else {
                    current_mode = Some(mode_part.to_string());
                }
            }
        }

        if line.contains("🎨 Switched color theme") {
            theme_switches += 1;
        }
    }

    UIState {
        professional_ui_active: professional_ui,
        demo_mode_activated: demo_mode,
        audio_capture_working: audio_capture,
        current_visualization_mode: current_mode,
        theme_switches,
        mode_switches,
    }
}

fn analyze_performance(lines: &[&str]) -> PerformanceMetrics {
    let mut fps_measurements = Vec::new();
    let mut audio_packets = 0;
    let mut warnings = 0;
    let mut errors = 0;

    let fps_re = Regex::new(r"FPS[:\s]+(\d+\.?\d*)").unwrap();
    let audio_re = Regex::new(r"Audio packets received: (\d+)").unwrap();

    for line in lines {
        // Extract FPS values
        if let Some(cap) = fps_re.captures(line) {
            if let Ok(fps) = cap[1].parse::<f32>() {
                if fps > 0.0 && fps < 10000.0 {
                    // Sanity check
                    fps_measurements.push(fps);
                }
            }
        }

        // Count audio packets
        if let Some(cap) = audio_re.captures(line) {
            if let Ok(packets) = cap[1].parse::<usize>() {
                audio_packets = packets.max(audio_packets);
            }
        }

        // Count warnings and errors
        if line.contains("warning:") {
            warnings += 1;
        }
        if line.contains("error:") || line.contains("ERROR") || line.contains("panic") {
            errors += 1;
        }
    }

    let average_fps = if !fps_measurements.is_empty() {
        fps_measurements.iter().sum::<f32>() / fps_measurements.len() as f32
    } else {
        0.0
    };

    let max_fps = fps_measurements.iter().cloned().fold(0.0f32, f32::max);
    let min_fps = fps_measurements
        .iter()
        .cloned()
        .fold(f32::INFINITY, f32::min);
    let min_fps = if min_fps == f32::INFINITY {
        0.0
    } else {
        min_fps
    };

    PerformanceMetrics {
        fps_measurements,
        average_fps,
        max_fps,
        min_fps,
        audio_packets_received: audio_packets,
        compilation_warnings: warnings,
        errors_found: errors,
    }
}

fn analyze_visual_quality(lines: &[&str]) -> VisualQuality {
    let mut color_sequences = 0;
    let mut frame_changes = 0;
    let mut character_variety = std::collections::HashSet::new();
    let mut audio_responses = 0;

    for line in lines {
        // Count ANSI color escape sequences
        if line.contains("\x1b[") || line.contains("Color::Rgb") {
            color_sequences += 1;
        }

        // Count frame indicators
        if line.contains("render") || line.contains("draw") || line.contains("frame") {
            frame_changes += 1;
        }

        // Count unique characters (for rendering complexity)
        for c in line.chars() {
            if !c.is_ascii_alphanumeric() && !c.is_whitespace() {
                character_variety.insert(c);
            }
        }

        // Count audio responsiveness indicators
        if line.contains("magnitude") || line.contains("Audio Stats") {
            audio_responses += 1;
        }
    }

    let total_lines = lines.len() as f32;

    VisualQuality {
        color_variety_score: (color_sequences as f32 / total_lines).min(1.0),
        animation_smoothness: (frame_changes as f32 / total_lines * 10.0).min(1.0),
        rendering_complexity: (character_variety.len() as f32 / 50.0).min(1.0),
        responsiveness_score: (audio_responses as f32 / total_lines * 100.0).min(1.0),
    }
}

fn find_issues(report: &VisualAnalysisReport) -> Vec<Issue> {
    let mut issues = Vec::new();

    // Check for critical issues
    if !report.ui_state.audio_capture_working {
        issues.push(Issue {
            severity: IssueSeverity::Critical,
            category: IssueCategory::AudioPipeline,
            description: "Audio capture not working - no audio input detected".to_string(),
            line_number: None,
            suggestion: Some("Check audio device permissions and configuration".to_string()),
        });
    }

    if report.performance_metrics.errors_found > 0 {
        issues.push(Issue {
            severity: IssueSeverity::Critical,
            category: IssueCategory::Compilation,
            description: format!("Errors found: {}", report.performance_metrics.errors_found),
            line_number: None,
            suggestion: Some("Fix compilation errors before proceeding".to_string()),
        });
    }

    // Check for high-priority issues
    if !report.ui_state.professional_ui_active {
        issues.push(Issue {
            severity: IssueSeverity::High,
            category: IssueCategory::UserInterface,
            description: "Professional UI not activated - using simple interface".to_string(),
            line_number: None,
            suggestion: Some("Ensure professional UI is set as default".to_string()),
        });
    }

    if report.performance_metrics.average_fps < 30.0 {
        issues.push(Issue {
            severity: IssueSeverity::High,
            category: IssueCategory::Performance,
            description: format!("Low FPS: {:.1}", report.performance_metrics.average_fps),
            line_number: None,
            suggestion: Some("Optimize rendering pipeline for better performance".to_string()),
        });
    }

    // Check for medium-priority issues
    if report.visual_quality.color_variety_score < 0.3 {
        issues.push(Issue {
            severity: IssueSeverity::Medium,
            category: IssueCategory::VisualQuality,
            description: "Low color variety in visualization".to_string(),
            line_number: None,
            suggestion: Some("Enhance color palette and gradient usage".to_string()),
        });
    }

    if report.visual_quality.responsiveness_score < 0.5 {
        issues.push(Issue {
            severity: IssueSeverity::Medium,
            category: IssueCategory::AudioPipeline,
            description: "Low audio responsiveness detected".to_string(),
            line_number: None,
            suggestion: Some("Improve audio processing frequency and sensitivity".to_string()),
        });
    }

    // Check for low-priority issues
    if report.performance_metrics.compilation_warnings > 10 {
        issues.push(Issue {
            severity: IssueSeverity::Low,
            category: IssueCategory::Compilation,
            description: format!(
                "Many compilation warnings: {}",
                report.performance_metrics.compilation_warnings
            ),
            line_number: None,
            suggestion: Some("Clean up unused imports and variables".to_string()),
        });
    }

    issues
}

fn generate_recommendations(report: &VisualAnalysisReport) -> Vec<String> {
    let mut recommendations = Vec::new();

    // Performance recommendations
    if report.performance_metrics.average_fps > 1000.0 {
        recommendations.push("Consider adding frame rate limiting to reduce CPU usage".to_string());
    }

    if report.performance_metrics.average_fps < 60.0 && report.performance_metrics.average_fps > 0.0
    {
        recommendations
            .push("Optimize rendering to achieve 60+ FPS for smooth visuals".to_string());
    }

    // Visual quality recommendations
    if report.visual_quality.color_variety_score > 0.8 {
        recommendations
            .push("Excellent color usage - consider this the visual standard".to_string());
    } else if report.visual_quality.color_variety_score < 0.4 {
        recommendations
            .push("Enhance color variety with gradients and dynamic palettes".to_string());
    }

    // UI/UX recommendations
    if report.ui_state.demo_mode_activated {
        recommendations
            .push("Demo mode working well - good fallback for no audio input".to_string());
    }

    if report.ui_state.mode_switches > 3 {
        recommendations.push("Good interactivity - users are actively switching modes".to_string());
    }

    // Audio pipeline recommendations
    if report.performance_metrics.audio_packets_received > 100 {
        recommendations
            .push("Audio pipeline performing well with consistent data flow".to_string());
    } else if report.performance_metrics.audio_packets_received > 0 {
        recommendations
            .push("Audio capture working but may need optimization for consistency".to_string());
    }

    recommendations
}

fn print_report(report: &VisualAnalysisReport) {
    println!("🔍 VISUAL OUTPUT ANALYSIS REPORT");
    println!("================================");
    println!();

    // Session Info
    println!("📊 Session Information:");
    println!("  Duration: {:.1}s", report.session_info.duration_seconds);
    println!("  Lines captured: {}", report.session_info.total_lines);
    println!("  File size: {} bytes", report.session_info.file_size_bytes);
    println!();

    // UI State
    println!("🖥️ UI State:");
    println!(
        "  Professional UI: {}",
        if report.ui_state.professional_ui_active {
            "✅ Active"
        } else {
            "❌ Inactive"
        }
    );
    println!(
        "  Demo Mode: {}",
        if report.ui_state.demo_mode_activated {
            "✅ Activated"
        } else {
            "⚪ Not activated"
        }
    );
    println!(
        "  Audio Capture: {}",
        if report.ui_state.audio_capture_working {
            "✅ Working"
        } else {
            "❌ Failed"
        }
    );
    if let Some(ref mode) = report.ui_state.current_visualization_mode {
        println!("  Current Mode: {}", mode);
    }
    println!("  Mode Switches: {}", report.ui_state.mode_switches);
    println!("  Theme Switches: {}", report.ui_state.theme_switches);
    println!();

    // Performance Metrics
    println!("⚡ Performance Metrics:");
    if report.performance_metrics.average_fps > 0.0 {
        println!(
            "  Average FPS: {:.1}",
            report.performance_metrics.average_fps
        );
        println!("  Max FPS: {:.1}", report.performance_metrics.max_fps);
        println!("  Min FPS: {:.1}", report.performance_metrics.min_fps);
    } else {
        println!("  FPS: No measurements captured");
    }
    println!(
        "  Audio Packets: {}",
        report.performance_metrics.audio_packets_received
    );
    println!(
        "  Warnings: {}",
        report.performance_metrics.compilation_warnings
    );
    println!("  Errors: {}", report.performance_metrics.errors_found);
    println!();

    // Visual Quality
    println!("🎨 Visual Quality:");
    println!(
        "  Color Variety: {:.2}/1.0 ({})",
        report.visual_quality.color_variety_score,
        quality_rating(report.visual_quality.color_variety_score)
    );
    println!(
        "  Animation Smoothness: {:.2}/1.0 ({})",
        report.visual_quality.animation_smoothness,
        quality_rating(report.visual_quality.animation_smoothness)
    );
    println!(
        "  Rendering Complexity: {:.2}/1.0 ({})",
        report.visual_quality.rendering_complexity,
        quality_rating(report.visual_quality.rendering_complexity)
    );
    println!(
        "  Audio Responsiveness: {:.2}/1.0 ({})",
        report.visual_quality.responsiveness_score,
        quality_rating(report.visual_quality.responsiveness_score)
    );
    println!();

    // Issues
    if !report.issues_found.is_empty() {
        println!("🚨 Issues Found:");
        for (i, issue) in report.issues_found.iter().enumerate() {
            let severity_icon = match issue.severity {
                IssueSeverity::Critical => "🔴",
                IssueSeverity::High => "🟡",
                IssueSeverity::Medium => "🟠",
                IssueSeverity::Low => "🔵",
            };
            println!(
                "  {}. {} {:?}: {}",
                i + 1,
                severity_icon,
                issue.severity,
                issue.description
            );
            if let Some(ref suggestion) = issue.suggestion {
                println!("     💡 {}", suggestion);
            }
        }
        println!();
    }

    // Recommendations
    if !report.recommendations.is_empty() {
        println!("💡 Recommendations:");
        for (i, rec) in report.recommendations.iter().enumerate() {
            println!("  {}. {}", i + 1, rec);
        }
        println!();
    }

    // Overall Assessment
    let critical_issues = report
        .issues_found
        .iter()
        .filter(|i| i.severity == IssueSeverity::Critical)
        .count();
    let high_issues = report
        .issues_found
        .iter()
        .filter(|i| i.severity == IssueSeverity::High)
        .count();

    println!("🎯 Overall Assessment:");
    if critical_issues == 0 && high_issues == 0 {
        println!("  ✅ EXCELLENT - Ready for production use");
    } else if critical_issues == 0 && high_issues <= 2 {
        println!("  ✅ GOOD - Minor improvements recommended");
    } else if critical_issues == 0 {
        println!("  ⚠️ NEEDS WORK - Several improvements needed");
    } else {
        println!("  ❌ CRITICAL ISSUES - Must fix before release");
    }
}

fn quality_rating(score: f32) -> &'static str {
    if score >= 0.8 {
        "Excellent"
    } else if score >= 0.6 {
        "Good"
    } else if score >= 0.4 {
        "Fair"
    } else if score >= 0.2 {
        "Poor"
    } else {
        "Very Poor"
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <captured_output_file>", args[0]);
        std::process::exit(1);
    }

    let file_path = Path::new(&args[1]);
    if !file_path.exists() {
        eprintln!("❌ File not found: {:?}", file_path);
        std::process::exit(1);
    }

    match analyze_visual_output(file_path) {
        Ok(report) => {
            print_report(&report);

            // Save detailed report
            let report_json = serde_json::to_string_pretty(&report)?;
            let report_file = file_path.with_extension("analysis.json");
            fs::write(&report_file, report_json)?;
            println!("📄 Detailed report saved to: {:?}", report_file);
        }
        Err(e) => {
            error!("Failed to analyze output: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
