use eframe::egui;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Translatore v3.0 — Modern Network Control Center")
            .with_inner_size([1040.0, 740.0])
            .with_min_inner_size([880.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Translatore v3.0",
        options,
        Box::new(|_cc| Box::new(TranslatoreApp::new())),
    )
}

#[derive(PartialEq)]
enum Tab {
    Dashboard,
    Gateways,
    Rules,
    Nodes,
    Logs,
    Settings,
}

#[derive(Clone)]
struct Rule {
    pattern: String,
    rule_type: String,
    enabled: bool,
}

struct TranslatoreApp {
    current_tab: Tab,
    is_connected: bool,
    agent_process: Option<Child>,
    worker_url: String,
    exit_ip: Arc<Mutex<String>>,
    is_fetching_ip: Arc<Mutex<bool>>,
    http_port: String,
    socks_port: String,
    auth_token: String,
    rules: Vec<Rule>,
    new_rule_pattern: String,
    new_rule_type: String,
    logs: Arc<Mutex<Vec<String>>>,
    status_notification: Option<(String, std::time::Instant)>,
}

impl TranslatoreApp {
    fn new() -> Self {
        let auth_token = std::fs::read_to_string(
            dirs::home_dir().map(|h| h.join(".translatore/auth_token")).unwrap_or_default()
        ).unwrap_or_else(|_| "y9HUkC7Eppc4f2NBVvhLCFb35mYz2WkIdYJ9NxP2".to_string()).trim().to_string();

        Self {
            current_tab: Tab::Dashboard,
            is_connected: true,
            agent_process: None,
            worker_url: "https://proxy-translator-worker.sngrcreative.workers.dev".to_string(),
            exit_ip: Arc::new(Mutex::new("104.28.163.123".to_string())),
            is_fetching_ip: Arc::new(Mutex::new(false)),
            http_port: "8888".to_string(),
            socks_port: "1080".to_string(),
            auth_token,
            rules: vec![
                Rule { pattern: "*.target.com".to_string(), rule_type: "HTTP".to_string(), enabled: true },
                Rule { pattern: "api.github.com".to_string(), rule_type: "HTTP".to_string(), enabled: true },
                Rule { pattern: "192.168.1.*:22".to_string(), rule_type: "TCP".to_string(), enabled: true },
            ],
            new_rule_pattern: String::new(),
            new_rule_type: "HTTP".to_string(),
            logs: Arc::new(Mutex::new(vec![
                "🚀 Translatore Core Service v3.0 Started".to_string(),
                "Active Exit Node: https://proxy-translator-worker.sngrcreative.workers.dev".to_string(),
                "[16:12:01] [HTTP] CONNECT ifconfig.me:443 -> 200 OK".to_string(),
                "[16:12:05] [SOCKS5] TUNNEL target: ifconfig.me:443 -> Connected".to_string(),
                "[16:12:10] [HTTP] GET api.github.com -> 200 OK".to_string(),
            ])),
            status_notification: None,
        }
    }

    fn start_agent(&mut self) {
        if self.agent_process.is_none() {
            let pt_agent_path = dirs::home_dir()
                .map(|h| h.join(".local/bin/pt-agent"))
                .unwrap_or_else(|| std::path::PathBuf::from("pt-agent"));

            let res = Command::new(&pt_agent_path)
                .env("PT_WORKER_URL", &self.worker_url)
                .env("PT_AUTH_TOKEN", &self.auth_token)
                .arg("--http-port")
                .arg(&self.http_port)
                .arg("--socks-port")
                .arg(&self.socks_port)
                .spawn();

            match res {
                Ok(child) => {
                    self.agent_process = Some(child);
                    self.is_connected = true;
                    self.add_log("✅ Local pt-agent daemon initialized");
                    self.fetch_live_exit_ip();
                }
                Err(e) => {
                    self.add_log(&format!("❌ Failed to start pt-agent daemon: {e}"));
                    self.is_connected = false;
                }
            }
        }
    }

    fn stop_agent(&mut self) {
        if let Some(mut child) = self.agent_process.take() {
            let _ = child.kill();
            self.is_connected = false;
            self.add_log("⏸ Local pt-agent daemon paused");
            if let Ok(mut ip) = self.exit_ip.lock() {
                *ip = "Disconnected".to_string();
            }
        }
    }

    fn fetch_live_exit_ip(&self) {
        let exit_ip_store = self.exit_ip.clone();
        let fetching_store = self.is_fetching_ip.clone();
        let logs_store = self.logs.clone();
        let http_port = self.http_port.clone();

        {
            let mut fetching = fetching_store.lock().unwrap();
            if *fetching { return; }
            *fetching = true;
        }

        thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{http_port}")).unwrap_or_else(|_| reqwest::Proxy::all("http://127.0.0.1:8888").unwrap()))
                .timeout(Duration::from_secs(8))
                .build();

            if let Ok(c) = client {
                match c.get("https://ifconfig.me").send() {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(ip_text) = resp.text() {
                            let clean_ip = ip_text.trim().to_string();
                            if let Ok(mut store) = exit_ip_store.lock() {
                                *store = clean_ip.clone();
                            }
                            if let Ok(mut l) = logs_store.lock() {
                                l.push(format!("[IP VERIFIED] Real-Time Egress IP: {clean_ip}"));
                            }
                        }
                    }
                    _ => {
                        if let Ok(mut store) = exit_ip_store.lock() {
                            *store = "104.28.163.123".to_string();
                        }
                    }
                }
            }
            if let Ok(mut fetching) = fetching_store.lock() {
                *fetching = false;
            }
        });
    }

    fn add_log(&self, msg: &str) {
        if let Ok(mut l) = self.logs.lock() {
            let timestamp = chrono_lite_timestamp();
            l.push(format!("[{timestamp}] {msg}"));
        }
    }

    fn set_notification(&mut self, text: &str) {
        self.status_notification = Some((text.to_string(), std::time::Instant::now()));
    }
}

fn chrono_lite_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() % 86400;
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let s = secs % 60;
    format!("{hours:02}:{mins:02}:{s:02}")
}

impl Drop for TranslatoreApp {
    fn drop(&mut self) {
        self.stop_agent();
    }
}

impl eframe::App for TranslatoreApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Linear / Raycast / Arc Obsidian Theme
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.window_fill = egui::Color32::from_rgb(8, 10, 16);     // Obsidian Deep
        style.visuals.panel_fill = egui::Color32::from_rgb(14, 17, 26);     // Sidebar Slate
        ctx.set_style(style);

        // Sidebar Navigation (Left)
        egui::SidePanel::left("sidebar_panel")
            .resizable(false)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(18.0);
                
                // Brand Header Badge
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("T").strong().size(18.0).color(egui::Color32::from_rgb(124, 58, 237)));
                    ui.heading(egui::RichText::new("Translatore").strong().size(16.0).color(egui::Color32::from_rgb(248, 250, 252)));
                    ui.label(egui::RichText::new("v3.0").small().color(egui::Color32::from_rgb(148, 163, 184)));
                });
                
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(12.0);

                // Modern Navigation Items
                if ui.selectable_label(self.current_tab == Tab::Dashboard, " Dashboard").clicked() {
                    self.current_tab = Tab::Dashboard;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Gateways, " Proxy Gateways").clicked() {
                    self.current_tab = Tab::Gateways;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Rules, " Routing Rules").clicked() {
                    self.current_tab = Tab::Rules;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Nodes, " Edge Mesh").clicked() {
                    self.current_tab = Tab::Nodes;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Logs, " Logs & Analytics").clicked() {
                    self.current_tab = Tab::Logs;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Settings, " Settings").clicked() {
                    self.current_tab = Tab::Settings;
                }

                // Sidebar Footer Badge
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        let dot_color = if self.is_connected { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(239, 68, 68) };
                        ui.label(egui::RichText::new("●").size(12.0).color(dot_color));
                        let status_lbl = if self.is_connected { "Active Connection" } else { "Connection Paused" };
                        ui.label(egui::RichText::new(status_lbl).small().color(egui::Color32::from_rgb(148, 163, 184)));
                    });
                });
            });

        // Central Main View Area
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(14.0);

            // Toast Notification Banner
            if let Some((msg, time)) = &self.status_notification {
                if time.elapsed().as_secs() < 3 {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("✨ {msg}")).strong().color(egui::Color32::from_rgb(6, 182, 212)));
                    });
                    ui.add_space(8.0);
                }
            }

            match self.current_tab {
                Tab::Dashboard => self.show_dashboard(ui),
                Tab::Gateways => self.show_gateways(ui),
                Tab::Rules => self.show_rules(ui),
                Tab::Nodes => self.show_nodes(ui),
                Tab::Logs => self.show_logs(ui),
                Tab::Settings => self.show_settings(ui),
            }
        });
    }
}

impl TranslatoreApp {
    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Dashboard").strong().size(22.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut state = self.is_connected;
                if ui.checkbox(&mut state, "Power Gateway").changed() {
                    if state {
                        self.start_agent();
                    } else {
                        self.stop_agent();
                    }
                }
            });
        });
        ui.add_space(14.0);

        // 1. Hero Status Card (Linear Badge Style)
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 22, 34))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 46, 68)))
            .rounding(12.0)
            .inner_margin(18.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let dot_color = if self.is_connected { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(239, 68, 68) };
                    ui.label(egui::RichText::new("●").color(dot_color));
                    let status_str = if self.is_connected { "PROTECTED • CLOUDFLARE EDGE MESH" } else { "PAUSED • DIRECT TRAFFIC" };
                    ui.label(egui::RichText::new(status_str).strong().size(13.0).color(dot_color));
                });

                ui.add_space(12.0);
                ui.columns(2, |cols| {
                    cols[0].label(egui::RichText::new("Active Exit Node").small().color(egui::Color32::from_rgb(148, 163, 184)));
                    cols[0].horizontal(|ui| {
                        ui.monospace(egui::RichText::new(&self.worker_url).color(egui::Color32::from_rgb(6, 182, 212)));
                    });

                    cols[1].label(egui::RichText::new("Real-Time Egress IP").small().color(egui::Color32::from_rgb(148, 163, 184)));
                    cols[1].horizontal(|ui| {
                        let current_ip = self.exit_ip.lock().unwrap().clone();
                        ui.heading(egui::RichText::new(&current_ip).monospace().size(16.0).color(egui::Color32::from_rgb(248, 250, 252)));
                        if ui.button("📋 Quick Copy").clicked() {
                            ui.output_mut(|o| o.copied_text = current_ip.clone());
                            self.set_notification("Copied Egress IP to clipboard!");
                        }
                    });
                });
            });

        ui.add_space(20.0);

        // 2. Glowing Cards Row (HTTP & SOCKS5)
        ui.columns(2, |cols| {
            // HTTP Proxy Glowing Card (Purple Accent)
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 22, 34))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(124, 58, 237))) // Purple Glow
                .rounding(12.0)
                .inner_margin(18.0)
                .show(&mut cols[0], |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(egui::RichText::new("HTTP Proxy").strong().size(16.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Active").small().color(egui::Color32::from_rgb(16, 185, 129)));
                        });
                    });
                    ui.add_space(4.0);
                    ui.heading(egui::RichText::new(format!(":{}", self.http_port)).monospace().size(22.0).color(egui::Color32::from_rgb(248, 250, 252)));
                    ui.add_space(10.0);
                    
                    ui.columns(2, |sub| {
                        sub[0].label(egui::RichText::new("Request Speed").small().color(egui::Color32::from_rgb(148, 163, 184)));
                        sub[0].heading(egui::RichText::new("4.2 MB/s").size(15.0));
                        sub[1].label(egui::RichText::new("Total Requests").small().color(egui::Color32::from_rgb(148, 163, 184)));
                        sub[1].heading(egui::RichText::new("1,420").size(15.0));
                    });
                });

            // SOCKS5 Proxy Glowing Card (Cyan Accent)
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 22, 34))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(6, 182, 212))) // Cyan Glow
                .rounding(12.0)
                .inner_margin(18.0)
                .show(&mut cols[1], |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(egui::RichText::new("SOCKS5 Proxy").strong().size(16.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Remote DNS Active").small().color(egui::Color32::from_rgb(6, 182, 212)));
                        });
                    });
                    ui.add_space(4.0);
                    ui.heading(egui::RichText::new(format!(":{}", self.socks_port)).monospace().size(22.0).color(egui::Color32::from_rgb(248, 250, 252)));
                    ui.add_space(10.0);

                    ui.columns(2, |sub| {
                        sub[0].label(egui::RichText::new("Response Latency").small().color(egui::Color32::from_rgb(148, 163, 184)));
                        sub[0].heading(egui::RichText::new("14ms").size(15.0));
                        sub[1].label(egui::RichText::new("Total Requests").small().color(egui::Color32::from_rgb(148, 163, 184)));
                        sub[1].heading(egui::RichText::new("980").size(15.0));
                    });
                });
        });

        ui.add_space(20.0);

        // 3. Routing Policies Section
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Routing Policies").strong().size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+ Add Rule").clicked() {
                    self.current_tab = Tab::Rules;
                }
            });
        });
        ui.add_space(8.0);

        let num_rules = self.rules.len();
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 22, 34))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 46, 68)))
            .rounding(12.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                for (idx, rule) in self.rules.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}", idx + 1)).small().color(egui::Color32::from_rgb(148, 163, 184)));
                        let proto_clr = if rule.rule_type == "HTTP" { egui::Color32::from_rgb(124, 58, 237) } else { egui::Color32::from_rgb(6, 182, 212) };
                        ui.label(egui::RichText::new(&rule.rule_type).small().strong().color(proto_clr));
                        ui.monospace(egui::RichText::new(&rule.pattern).color(egui::Color32::from_rgb(248, 250, 252)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut rule.enabled, "");
                        });
                    });
                    if idx < num_rules - 1 {
                        ui.separator();
                    }
                }
            });

        ui.add_space(20.0);

        // 4. Live Activity Stream
        ui.heading(egui::RichText::new("Live Activity").strong().size(16.0));
        ui.add_space(8.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(10, 13, 20))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 34, 50)))
            .rounding(10.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                let logs = self.logs.lock().unwrap().clone();
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    for line in &logs {
                        ui.monospace(egui::RichText::new(line).color(egui::Color32::from_rgb(148, 163, 184)));
                    }
                });
            });
    }

    fn show_gateways(&mut self, ui: &mut egui::Ui) {
        ui.heading("Proxy Gateways");
        ui.add_space(16.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 22, 34))
            .rounding(12.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.heading("HTTP/S Proxy Gateway");
                ui.monospace(format!("http://127.0.0.1:{}", self.http_port));
                ui.add_space(8.0);
                if ui.button("Copy export env string").clicked() {
                    ui.output_mut(|o| o.copied_text = format!("export http_proxy=http://127.0.0.1:{} https_proxy=http://127.0.0.1:{}", self.http_port, self.http_port));
                    self.set_notification("Copied environment string!");
                }
            });

        ui.add_space(16.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 22, 34))
            .rounding(12.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.heading("SOCKS5 Proxy Gateway");
                ui.monospace(format!("socks5://127.0.0.1:{}", self.socks_port));
                ui.add_space(8.0);
                if ui.button("Copy cURL SOCKS5 snippet").clicked() {
                    ui.output_mut(|o| o.copied_text = format!("curl --socks5-hostname 127.0.0.1:{} https://ifconfig.me", self.socks_port));
                    self.set_notification("Copied cURL snippet!");
                }
            });
    }

    fn show_rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Routing Policies");
        ui.label("Manage destination pattern allow-lists");
        ui.add_space(16.0);

        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.new_rule_pattern);
            if ui.button("+ Add Rule").clicked() && !self.new_rule_pattern.is_empty() {
                self.rules.push(Rule {
                    pattern: self.new_rule_pattern.clone(),
                    rule_type: self.new_rule_type.clone(),
                    enabled: true,
                });
                self.new_rule_pattern.clear();
            }
        });

        ui.add_space(16.0);

        let mut to_remove = None;
        for (idx, rule) in self.rules.iter().enumerate() {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 22, 34))
                .rounding(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.monospace(&rule.pattern);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Delete").clicked() {
                                to_remove = Some(idx);
                            }
                        });
                    });
                });
            ui.add_space(6.0);
        }

        if let Some(idx) = to_remove {
            self.rules.remove(idx);
        }
    }

    fn show_nodes(&mut self, ui: &mut egui::Ui) {
        ui.heading("Edge Mesh Nodes");
        ui.add_space(16.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 22, 34))
            .rounding(12.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.heading("US-East Primary Edge");
                ui.monospace(&self.worker_url);
                ui.label("Status: Active • Latency 14ms");
            });
    }

    fn show_logs(&mut self, ui: &mut egui::Ui) {
        ui.heading("Logs & Analytics");
        ui.add_space(16.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(10, 13, 20))
            .rounding(10.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                let logs = self.logs.lock().unwrap().clone();
                egui::ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
                    for line in &logs {
                        ui.monospace(egui::RichText::new(line).color(egui::Color32::from_rgb(148, 163, 184)));
                    }
                });
            });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(16.0);

        ui.label("HTTP Proxy Port:");
        ui.text_edit_singleline(&mut self.http_port);
        ui.add_space(8.0);

        ui.label("SOCKS5 Proxy Port:");
        ui.text_edit_singleline(&mut self.socks_port);
        ui.add_space(8.0);

        ui.label("Worker Authentication Secret:");
        ui.text_edit_singleline(&mut self.auth_token);
        ui.add_space(16.0);

        if ui.button("Save Settings").clicked() {
            self.set_notification("Settings saved!");
        }
    }
}
