use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Translatore v3.0 — Desktop Control Center")
            .with_inner_size([1024.0, 720.0])
            .with_min_inner_size([800.0, 550.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Translatore v3.0",
        options,
        Box::new(|_cc| Box::new(TranslatoreApp::default())),
    )
}

#[derive(PartialEq)]
enum Tab {
    Dashboard,
    Rules,
    Nodes,
    Logs,
    Settings,
}

struct Rule {
    pattern: String,
    rule_type: String,
}

struct TranslatoreApp {
    current_tab: Tab,
    is_connected: bool,
    worker_url: String,
    exit_ip: String,
    http_port: String,
    socks_port: String,
    auth_token: String,
    http_count: usize,
    socks_count: usize,
    rules: Vec<Rule>,
    new_rule_pattern: String,
    new_rule_type: String,
    logs: Vec<String>,
}

impl Default for TranslatoreApp {
    fn default() -> Self {
        Self {
            current_tab: Tab::Dashboard,
            is_connected: true,
            worker_url: "https://proxy-translator-worker.sngrcreative.workers.dev".to_string(),
            exit_ip: "104.28.163.123 (Cloudflare Edge)".to_string(),
            http_port: "8888".to_string(),
            socks_port: "1080".to_string(),
            auth_token: "y9HUkC7Eppc4f2NBVvhLCFb35mYz2WkIdYJ9NxP2".to_string(),
            http_count: 1247,
            socks_count: 856,
            rules: vec![
                Rule { pattern: "*.target.com".to_string(), rule_type: "HTTP".to_string() },
                Rule { pattern: "api.github.com".to_string(), rule_type: "HTTP".to_string() },
                Rule { pattern: "192.168.1.*:22".to_string(), rule_type: "TCP".to_string() },
            ],
            new_rule_pattern: String::new(),
            new_rule_type: "HTTP".to_string(),
            logs: vec![
                "🚀 Proxy Translator Agent v3.0 initialized".to_string(),
                "   HTTP proxy: 127.0.0.1:8888".to_string(),
                "   SOCKS5 proxy (Remote DNS): 127.0.0.1:1080".to_string(),
                "[11:47:59] [HTTP] CONNECT ifconfig.me:443 -> 200 OK".to_string(),
                "[11:48:02] [SOCKS5] TUNNEL target: ifconfig.me:443 -> Connected".to_string(),
            ],
        }
    }
}

impl eframe::App for TranslatoreApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Dark theme styling
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.window_fill = egui::Color32::from_rgb(11, 15, 25);
        style.visuals.panel_fill = egui::Color32::from_rgb(7, 10, 17);
        ctx.set_style(style);

        // Sidebar Panel
        egui::SidePanel::left("sidebar_panel")
            .resizable(false)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(16.0);
                
                // Brand Header
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new("⚡ Translatore").strong().color(egui::Color32::from_rgb(139, 92, 246)));
                    ui.label(egui::RichText::new("v3.0").small().color(egui::Color32::from_rgb(156, 163, 175)));
                });
                
                ui.add_space(24.0);
                ui.separator();
                ui.add_space(12.0);

                // Nav items
                if ui.selectable_label(self.current_tab == Tab::Dashboard, "📊  Dashboard").clicked() {
                    self.current_tab = Tab::Dashboard;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Rules, "🎯  Routing Rules").clicked() {
                    self.current_tab = Tab::Rules;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Nodes, "🌐  Worker Nodes").clicked() {
                    self.current_tab = Tab::Nodes;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Logs, "💻  Live Logs").clicked() {
                    self.current_tab = Tab::Logs;
                }
                ui.add_space(4.0);
                if ui.selectable_label(self.current_tab == Tab::Settings, "⚙  Settings").clicked() {
                    self.current_tab = Tab::Settings;
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let dot_color = if self.is_connected { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(239, 68, 68) };
                        ui.label(egui::RichText::new("●").color(dot_color));
                        let status_lbl = if self.is_connected { "Agent Active" } else { "Agent Paused" };
                        ui.label(egui::RichText::new(status_lbl).small().color(egui::Color32::from_rgb(156, 163, 175)));
                    });
                });
            });

        // Central Content Area
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            match self.current_tab {
                Tab::Dashboard => self.show_dashboard(ui),
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
        ui.heading("System Dashboard");
        ui.add_space(12.0);

        // Status Panel
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 24, 38))
            .rounding(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (status_str, status_clr) = if self.is_connected {
                        ("CONNECTED", egui::Color32::from_rgb(16, 185, 129))
                    } else {
                        ("DISCONNECTED", egui::Color32::from_rgb(239, 68, 68))
                    };
                    ui.heading(egui::RichText::new(status_str).strong().color(status_clr));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.is_connected, "Enable Agent");
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Worker Node:");
                    ui.monospace(egui::RichText::new(&self.worker_url).color(egui::Color32::from_rgb(6, 182, 212)));
                });
                ui.horizontal(|ui| {
                    ui.label("Detected Exit IP:");
                    ui.monospace(egui::RichText::new(&self.exit_ip).color(egui::Color32::from_rgb(243, 244, 246)));
                });
            });

        ui.add_space(20.0);

        // Proxy Grid
        ui.columns(2, |cols| {
            // HTTP Card
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 24, 38))
                .rounding(10.0)
                .inner_margin(16.0)
                .show(&mut cols[0], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("HTTP PROXY").strong().color(egui::Color32::from_rgb(59, 130, 246)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Active").small().color(egui::Color32::from_rgb(16, 185, 129)));
                        });
                    });
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new(format!("127.0.0.1:{}", self.http_port)).monospace());
                    ui.label(format!("Requests processed: {}", self.http_count));
                });

            // SOCKS5 Card
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 24, 38))
                .rounding(10.0)
                .inner_margin(16.0)
                .show(&mut cols[1], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("SOCKS5 PROXY").strong().color(egui::Color32::from_rgb(139, 92, 246)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Remote DNS").small().color(egui::Color32::from_rgb(16, 185, 129)));
                        });
                    });
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new(format!("127.0.0.1:{}", self.socks_port)).monospace());
                    ui.label(format!("Requests processed: {}", self.socks_count));
                });
        });

        ui.add_space(20.0);
        ui.heading("Live Event Stream");
        ui.add_space(8.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(5, 7, 12))
            .rounding(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for line in &self.logs {
                        ui.monospace(egui::RichText::new(line).color(egui::Color32::from_rgb(156, 163, 175)));
                    }
                });
            });
    }

    fn show_rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Allow-list Routing Rules");
        ui.label("Pattern matching rules enforced on Cloudflare Worker KV");
        ui.add_space(16.0);

        // Presets
        ui.horizontal(|ui| {
            ui.label("Quick Presets:");
            if ui.button("🎯 Bug Bounty Preset").clicked() {
                self.rules = vec![
                    Rule { pattern: "*.target.com".to_string(), rule_type: "HTTP".to_string() },
                    Rule { pattern: "api.github.com".to_string(), rule_type: "HTTP".to_string() },
                    Rule { pattern: "192.168.1.*:22".to_string(), rule_type: "TCP".to_string() },
                ];
            }
            if ui.button("🔒 Strict Security").clicked() {
                self.rules = vec![
                    Rule { pattern: "ifconfig.me".to_string(), rule_type: "HTTP".to_string() },
                    Rule { pattern: "httpbin.org".to_string(), rule_type: "HTTP".to_string() },
                ];
            }
            if ui.button("⚡ Allow All (*)").clicked() {
                self.rules = vec![
                    Rule { pattern: "*".to_string(), rule_type: "ALL".to_string() },
                ];
            }
        });

        ui.add_space(16.0);

        // Add Rule Form
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.new_rule_pattern);
            if ui.button("+ Add Rule").clicked() && !self.new_rule_pattern.is_empty() {
                self.rules.push(Rule {
                    pattern: self.new_rule_pattern.clone(),
                    rule_type: self.new_rule_type.clone(),
                });
                self.new_rule_pattern.clear();
            }
        });

        ui.add_space(16.0);

        // Rules List
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 24, 38))
            .rounding(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.columns(4, |cols| {
                    cols[0].strong("Pattern");
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
                        cols[2].label(egui::RichText::new("Active").color(egui::Color32::from_rgb(16, 185, 129)));
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
        ui.heading("Cloudflare Worker Nodes");
        ui.label("Deployed worker deployments");
        ui.add_space(16.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 24, 38))
            .rounding(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("US-East Node (Production)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("ACTIVE").strong().color(egui::Color32::from_rgb(16, 185, 129)));
                    });
                });
                ui.monospace(&self.worker_url);
                ui.label("Ping Latency: 12ms");
            });
    }

    fn show_logs(&mut self, ui: &mut egui::Ui) {
        ui.heading("Live Output Logs");
        ui.add_space(12.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(5, 7, 12))
            .rounding(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
                    for line in &self.logs {
                        ui.monospace(egui::RichText::new(line).color(egui::Color32::from_rgb(156, 163, 175)));
                    }
                });
            });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings & Environment");
        ui.add_space(16.0);

        ui.label("HTTP Listener Port:");
        ui.text_edit_singleline(&mut self.http_port);
        ui.add_space(8.0);

        ui.label("SOCKS5 Listener Port:");
        ui.text_edit_singleline(&mut self.socks_port);
        ui.add_space(8.0);

        ui.label("Worker Authentication Secret (PROXY_AUTH):");
        ui.text_edit_singleline(&mut self.auth_token);
        ui.add_space(16.0);

        if ui.button("Save Configuration").clicked() {
            self.logs.push("Configuration saved.".to_string());
        }
    }
}
