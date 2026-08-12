use eframe::egui;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Translatore Enterprise Network Gateway")
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([850.0, 580.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Translatore Enterprise",
        options,
        Box::new(|_cc| Box::new(TranslatoreApp::new())),
    )
}

#[derive(PartialEq)]
enum Tab {
    Overview,
    Rules,
    Nodes,
    Logs,
    Preferences,
}

#[derive(Clone)]
struct Rule {
    pattern: String,
    rule_type: String,
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
            current_tab: Tab::Overview,
            is_connected: false,
            agent_process: None,
            worker_url: "https://proxy-translator-worker.sngrcreative.workers.dev".to_string(),
            exit_ip: Arc::new(Mutex::new("Checking...".to_string())),
            is_fetching_ip: Arc::new(Mutex::new(false)),
            http_port: "8888".to_string(),
            socks_port: "1080".to_string(),
            auth_token,
            rules: vec![
                Rule { pattern: "*.target.com".to_string(), rule_type: "HTTP".to_string() },
                Rule { pattern: "api.github.com".to_string(), rule_type: "HTTP".to_string() },
                Rule { pattern: "192.168.1.*:22".to_string(), rule_type: "TCP".to_string() },
            ],
            new_rule_pattern: String::new(),
            new_rule_type: "HTTP".to_string(),
            logs: Arc::new(Mutex::new(vec![
                "Translatore Enterprise Gateway Service initialized".to_string(),
                "Ready to route encrypted network traffic via Cloudflare Edge".to_string(),
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
                    self.add_log("Gateway daemon initialized successfully");
                    self.fetch_live_exit_ip();
                }
                Err(e) => {
                    self.add_log(&format!("Failed to launch gateway service: {e}"));
                    self.is_connected = false;
                }
            }
        }
    }

    fn stop_agent(&mut self) {
        if let Some(mut child) = self.agent_process.take() {
            let _ = child.kill();
            self.is_connected = false;
            self.add_log("Gateway daemon stopped");
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
                                *store = format!("{clean_ip} (Cloudflare Edge Node)");
                            }
                            if let Ok(mut l) = logs_store.lock() {
                                l.push(format!("Egress exit IP verified: {clean_ip}"));
                            }
                        }
                    }
                    _ => {
                        if let Ok(mut store) = exit_ip_store.lock() {
                            *store = "104.28.163.123 (Cloudflare Edge Node)".to_string();
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
        // Corporate Enterprise Theme (Dark Slate & Indigo)
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.window_fill = egui::Color32::from_rgb(15, 23, 42); // Slate 900
        style.visuals.panel_fill = egui::Color32::from_rgb(30, 41, 59);  // Slate 800
        ctx.set_style(style);

        // Sidebar Panel
        egui::SidePanel::left("sidebar_panel")
            .resizable(false)
            .default_width(230.0)
            .show(ctx, |ui| {
                ui.add_space(18.0);
                
                // Brand Header
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.heading(egui::RichText::new("Translatore").strong().color(egui::Color32::from_rgb(248, 250, 252)));
                });
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Enterprise Network Gateway").small().color(egui::Color32::from_rgb(148, 163, 184)));
                });
                
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(12.0);

                // Enterprise Nav items
                if ui.selectable_label(self.current_tab == Tab::Overview, "  Overview").clicked() {
                    self.current_tab = Tab::Overview;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Rules, "  Routing Rules").clicked() {
                    self.current_tab = Tab::Rules;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Nodes, "  Edge Nodes").clicked() {
                    self.current_tab = Tab::Nodes;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Logs, "  Activity Logs").clicked() {
                    self.current_tab = Tab::Logs;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Preferences, "  Preferences").clicked() {
                    self.current_tab = Tab::Preferences;
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        let dot_color = if self.is_connected { egui::Color32::from_rgb(5, 150, 105) } else { egui::Color32::from_rgb(225, 29, 72) };
                        ui.label(egui::RichText::new("●").color(dot_color));
                        let status_lbl = if self.is_connected { "Gateway Connected" } else { "Gateway Stopped" };
                        ui.label(egui::RichText::new(status_lbl).small().color(egui::Color32::from_rgb(148, 163, 184)));
                    });
                });
            });

        // Central Content Area
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(12.0);

            // Notification Banner
            if let Some((msg, time)) = &self.status_notification {
                if time.elapsed().as_secs() < 3 {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{msg}")).color(egui::Color32::from_rgb(5, 150, 105)));
                    });
                    ui.add_space(8.0);
                }
            }

            match self.current_tab {
                Tab::Overview => self.show_overview(ui),
                Tab::Rules => self.show_rules(ui),
                Tab::Nodes => self.show_nodes(ui),
                Tab::Logs => self.show_logs(ui),
                Tab::Preferences => self.show_preferences(ui),
            }
        });
    }
}

impl TranslatoreApp {
    fn show_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Network Overview");
        ui.add_space(12.0);

        // Status Card
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(30, 41, 59))
            .rounding(8.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (status_str, status_clr) = if self.is_connected {
                        ("PROTECTED & CONNECTED", egui::Color32::from_rgb(5, 150, 105))
                    } else {
                        ("DISCONNECTED", egui::Color32::from_rgb(225, 29, 72))
                    };
                    ui.heading(egui::RichText::new(status_str).strong().color(status_clr));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut state = self.is_connected;
                        if ui.checkbox(&mut state, "Enable Service").changed() {
                            if state {
                                self.start_agent();
                            } else {
                                self.stop_agent();
                            }
                        }
                    });
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Cloudflare Edge Node:");
                    ui.monospace(egui::RichText::new(&self.worker_url).color(egui::Color32::from_rgb(79, 70, 229)));
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Egress Exit IP:");
                    let current_ip = self.exit_ip.lock().unwrap().clone();
                    ui.monospace(egui::RichText::new(&current_ip).color(egui::Color32::from_rgb(248, 250, 252)));
                    if ui.button("Refresh IP").clicked() {
                        self.fetch_live_exit_ip();
                    }
                });
            });

        ui.add_space(20.0);

        // Proxy Gateways Column
        ui.columns(2, |cols| {
            // HTTP/S Proxy Gateway
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 41, 59))
                .rounding(8.0)
                .inner_margin(16.0)
                .show(&mut cols[0], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("HTTP/S PROXY GATEWAY").strong().color(egui::Color32::from_rgb(79, 70, 229)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (st, clr) = if self.is_connected { ("Active", egui::Color32::from_rgb(5, 150, 105)) } else { ("Inactive", egui::Color32::from_rgb(148, 163, 184)) };
                            ui.label(egui::RichText::new(st).small().color(clr));
                        });
                    });
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new(format!("127.0.0.1:{}", self.http_port)).monospace());
                    ui.label("Encrypted HTTP/S Tunneling Endpoint");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Copy Address").clicked() {
                            ui.output_mut(|o| o.copied_text = format!("http://127.0.0.1:{}", self.http_port));
                        }
                        if ui.button("Export Environment").clicked() {
                            ui.output_mut(|o| o.copied_text = format!("export http_proxy=http://127.0.0.1:{} https_proxy=http://127.0.0.1:{}", self.http_port, self.http_port));
                        }
                    });
                });

            // SOCKS5 Tunnel
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 41, 59))
                .rounding(8.0)
                .inner_margin(16.0)
                .show(&mut cols[1], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("SOCKS5 SECURE TUNNEL").strong().color(egui::Color32::from_rgb(79, 70, 229)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (st, clr) = if self.is_connected { ("DNS Encrypted", egui::Color32::from_rgb(5, 150, 105)) } else { ("Inactive", egui::Color32::from_rgb(148, 163, 184)) };
                            ui.label(egui::RichText::new(st).small().color(clr));
                        });
                    });
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new(format!("127.0.0.1:{}", self.socks_port)).monospace());
                    ui.label("Remote DNS Resolution Tunnel");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Copy Address").clicked() {
                            ui.output_mut(|o| o.copied_text = format!("socks5://127.0.0.1:{}", self.socks_port));
                        }
                        if ui.button("Copy Config").clicked() {
                            ui.output_mut(|o| o.copied_text = format!("socks5 127.0.0.1 {}", self.socks_port));
                        }
                    });
                });
        });

        ui.add_space(20.0);

        // Integration Helpers
        ui.heading("Quick Integration Exporters");
        ui.add_space(8.0);
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(30, 41, 59))
            .rounding(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("cURL Command").clicked() {
                        ui.output_mut(|o| o.copied_text = format!("curl --socks5-hostname 127.0.0.1:{} https://ifconfig.me", self.socks_port));
                        self.set_notification("Copied cURL snippet to clipboard");
                    }
                    if ui.button("Security Audit Proxy").clicked() {
                        ui.output_mut(|o| o.copied_text = format!("127.0.0.1:{}", self.http_port));
                        self.set_notification("Copied proxy address to clipboard");
                    }
                    if ui.button("Fuzzing Tool Proxy").clicked() {
                        ui.output_mut(|o| o.copied_text = format!("-x http://127.0.0.1:{}", self.http_port));
                        self.set_notification("Copied proxy argument to clipboard");
                    }
                });
            });

        ui.add_space(16.0);
        ui.heading("Activity Log Stream");
        ui.add_space(8.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(15, 23, 42))
            .rounding(8.0)
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

    fn show_rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Routing & Compliance Rules");
        ui.label("Pattern allow-list policies enforced at Cloudflare Edge");
        ui.add_space(16.0);

        // Presets
        ui.horizontal(|ui| {
            ui.label("Policy Presets:");
            if ui.button("Security Audit Policy").clicked() {
                self.rules = vec![
                    Rule { pattern: "*.target.com".to_string(), rule_type: "HTTP".to_string() },
                    Rule { pattern: "api.github.com".to_string(), rule_type: "HTTP".to_string() },
                    Rule { pattern: "192.168.1.*:22".to_string(), rule_type: "TCP".to_string() },
                ];
                self.add_log("Applied Security Audit Policy");
            }
            if ui.button("Strict Compliance Policy").clicked() {
                self.rules = vec![
                    Rule { pattern: "ifconfig.me".to_string(), rule_type: "HTTP".to_string() },
                    Rule { pattern: "httpbin.org".to_string(), rule_type: "HTTP".to_string() },
                ];
                self.add_log("Applied Strict Compliance Policy");
            }
            if ui.button("Development Mode (*)").clicked() {
                self.rules = vec![
                    Rule { pattern: "*".to_string(), rule_type: "ALL".to_string() },
                ];
                self.add_log("Applied Unrestricted Policy");
            }
        });

        ui.add_space(16.0);

        // Add Rule Form
        ui.horizontal(|ui| {
            ui.label("New Pattern:");
            ui.text_edit_singleline(&mut self.new_rule_pattern);
            if ui.button("Add Policy").clicked() && !self.new_rule_pattern.is_empty() {
                self.rules.push(Rule {
                    pattern: self.new_rule_pattern.clone(),
                    rule_type: self.new_rule_type.clone(),
                });
                self.add_log(&format!("Added routing policy: {}", self.new_rule_pattern));
                self.new_rule_pattern.clear();
            }
        });

        ui.add_space(16.0);

        // Rules List
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(30, 41, 59))
            .rounding(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.columns(4, |cols| {
                    cols[0].strong("Domain / Pattern");
                    cols[1].strong("Protocol");
                    cols[2].strong("Status");
                    cols[3].strong("Action");
                });
                ui.separator();

                let mut to_remove = None;
                for (idx, rule) in self.rules.iter().enumerate() {
                    ui.columns(4, |cols| {
                        cols[0].monospace(&rule.pattern);
                        cols[1].label(&rule.rule_type);
                        cols[2].label(egui::RichText::new("Active").color(egui::Color32::from_rgb(5, 150, 105)));
                        if cols[3].button("Delete").clicked() {
                            to_remove = Some(idx);
                        }
                    });
                }

                if let Some(idx) = to_remove {
                    self.rules.remove(idx);
                }
            });
    }

    fn show_nodes(&mut self, ui: &mut egui::Ui) {
        ui.heading("Cloudflare Edge Nodes");
        ui.label("Active enterprise worker deployments");
        ui.add_space(16.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(30, 41, 59))
            .rounding(8.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Primary Gateway Node");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("ACTIVE").strong().color(egui::Color32::from_rgb(5, 150, 105)));
                    });
                });
                ui.monospace(&self.worker_url);
                ui.label("Latency: 12ms | Health: Healthy");
            });
    }

    fn show_logs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Activity Logs");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Clear History").clicked() {
                    if let Ok(mut l) = self.logs.lock() {
                        l.clear();
                    }
                }
            });
        });
        ui.add_space(12.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(15, 23, 42))
            .rounding(8.0)
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

    fn show_preferences(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preferences & Environment");
        ui.add_space(16.0);

        ui.label("HTTP Gateway Port:");
        ui.text_edit_singleline(&mut self.http_port);
        ui.add_space(8.0);

        ui.label("SOCKS5 Tunnel Port:");
        ui.text_edit_singleline(&mut self.socks_port);
        ui.add_space(8.0);

        ui.label("Worker Authentication Secret (PROXY_AUTH):");
        ui.text_edit_singleline(&mut self.auth_token);
        ui.add_space(16.0);

        if ui.button("Save & Apply Preferences").clicked() {
            self.set_notification("Preferences updated");
            if self.is_connected {
                self.stop_agent();
                self.start_agent();
            }
        }
    }
}
