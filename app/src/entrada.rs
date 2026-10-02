//! Tela de entrada: escolher um perfil para entrar ou criar um novo, em três
//! passos (nome e tema, contas de IA, primeiro projeto).

use eframe::egui::{self, RichText, vec2};

use crate::api;
use crate::dialogos::SeletorPasta;
use crate::tema::{self, Escolha, cores, texto_forte};

#[derive(Clone, Copy, PartialEq)]
enum Passo {
    Lista,
    Nome,
    Contas,
    Projeto,
}

/// A escolha de cada ferramenta no assistente.
struct Escolhida {
    ferramenta: api::Ferramenta,
    usar: bool,
    separada: bool,
}

pub struct Entrada {
    passo: Passo,
    perfis: Vec<api::Perfil>,
    erro: Option<String>,
    nome: String,
    tema: Escolha,
    ferramentas: Vec<Escolhida>,
    workspace: String,
    projeto: String,
    pasta: String,
    seletor: SeletorPasta,
    /// O perfil já criado, para não criar de novo se o passo do projeto falhar.
    criado: Option<api::Perfil>,
    workspace_criado: Option<i64>,
    /// Pede o foco do campo de nome uma vez só: pedir em todo quadro trava os eventos.
    focar_nome: bool,
}

impl Entrada {
    /// `criar` abre direto no assistente (ex.: "Criar perfil" no menu do perfil).
    pub fn new(criar: bool) -> Self {
        let (perfis, erro) = match api::perfis() {
            Ok(p) => (p, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let passo = if criar || perfis.is_empty() { Passo::Nome } else { Passo::Lista };
        Entrada {
            passo,
            perfis,
            erro,
            nome: String::new(),
            tema: Escolha::Escuro,
            ferramentas: Vec::new(),
            workspace: String::new(),
            projeto: String::new(),
            pasta: String::new(),
            seletor: SeletorPasta::default(),
            criado: None,
            workspace_criado: None,
            focar_nome: true,
        }
    }

    /// Desenha a entrada; devolve o perfil quando é para entrar nele.
    pub fn mostrar(&mut self, ui: &mut egui::Ui) -> Option<api::Perfil> {
        let largura = 560.0_f32.min(ui.available_width() - 40.0);
        let mut entrar = None;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.12).min(90.0));
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 150.0) / 2.0);
                let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), egui::Sense::hover());
                tema::logo(ui.painter(), r.center(), 14.0);
                ui.label(texto_forte("Colmeia", 24.0).color(cores().texto));
            });
            ui.add_space(20.0);
            ui.allocate_ui(vec2(largura, 0.0), |ui| {
                tema::moldura_janela().show(ui, |ui| {
                    ui.set_width(largura - 48.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        entrar = match self.passo {
                            Passo::Lista => self.lista(ui),
                            Passo::Nome => {
                                self.nome(ui);
                                None
                            }
                            Passo::Contas => {
                                self.contas(ui);
                                None
                            }
                            Passo::Projeto => self.projeto(ui),
                        };
                        if let Some(erro) = &self.erro {
                            ui.add_space(12.0);
                            ui.label(RichText::new(erro).color(cores().erro).size(13.0));
                        }
                    });
                });
            });
        });
        entrar
    }

    fn lista(&mut self, ui: &mut egui::Ui) -> Option<api::Perfil> {
        let p = cores();
        tema::cabecalho(ui, "Escolha um perfil", "Cada perfil tem os próprios projetos, contas de IA e tema.");
        ui.add_space(16.0);
        let mut escolhido = None;
        for perfil in &self.perfis {
            let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), egui::Sense::click());
            let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().rect(rect, 10, if resposta.hovered() { p.realce } else { p.superficie }, egui::Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
            // Inicial do perfil num círculo, como um avatar.
            let circulo = rect.left_center() + vec2(28.0, 0.0);
            ui.painter().circle_filled(circulo, 15.0, p.destaque.gamma_multiply(0.18));
            let inicial: String = perfil.nome.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            ui.painter().text(circulo, egui::Align2::CENTER_CENTER, inicial, tema::forte(14.0), p.destaque);
            ui.painter().text(rect.left_center() + vec2(54.0, 0.0), egui::Align2::LEFT_CENTER, &perfil.nome, tema::forte(15.0), p.texto);
            ui.painter().text(rect.right_center() - vec2(16.0, 0.0), egui::Align2::RIGHT_CENTER, "Entrar ›", egui::FontId::proportional(13.0), p.suave);
            if resposta.clicked() {
                escolhido = Some(perfil.clone());
            }
            ui.add_space(8.0);
        }
        ui.add_space(8.0);
        if tema::botao_secundario(ui, "+ Criar perfil").clicked() {
            self.passo = Passo::Nome;
            self.erro = None;
            self.focar_nome = true;
        }
        escolhido
    }

    fn nome(&mut self, ui: &mut egui::Ui) {
        tema::cabecalho(ui, "Criar perfil", "Passo 1 de 3 · Um perfil separa contextos, como trabalho, estudo e projetos pessoais.");
        ui.add_space(18.0);
        let resposta = tema::campo(ui, "Nome do perfil", &mut self.nome, "Ex.: Profissional");
        if std::mem::take(&mut self.focar_nome) {
            resposta.request_focus();
        }
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(RichText::new("Sugestões:").color(cores().suave).size(12.5));
            for sugestao in ["Profissional", "Estudo", "Pessoal"] {
                if tema::botao_secundario(ui, sugestao).clicked() {
                    self.nome = sugestao.into();
                }
            }
        });
        ui.add_space(16.0);
        ui.label(RichText::new("Tema").color(cores().suave).size(12.5));
        ui.add_space(2.0);
        let atual = Escolha::TODAS.iter().position(|e| *e == self.tema).unwrap_or(0);
        if let Some(i) = tema::segmentado(ui, &Escolha::TODAS.map(|e| e.nome()), atual) {
            self.tema = Escolha::TODAS[i];
            self.tema.aplicar(ui.ctx());
        }
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            if !self.perfis.is_empty() && tema::botao_secundario(ui, "Voltar").clicked() {
                self.passo = Passo::Lista;
                self.erro = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let nome = self.nome.trim().to_string();
                let continuar =
                    tema::botao_principal(ui, "Continuar", !nome.is_empty()).clicked() || (!nome.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if continuar {
                    // O núcleo também recusa nome repetido; conferir aqui evita perder os outros passos.
                    if self.perfis.iter().any(|p| p.nome.eq_ignore_ascii_case(&nome)) && self.criado.is_none() {
                        self.erro = Some(format!("Já existe um perfil chamado \"{nome}\"."));
                    } else {
                        self.erro = None;
                        self.carregar_ferramentas();
                        self.passo = Passo::Contas;
                    }
                }
            });
        });
    }

    fn carregar_ferramentas(&mut self) {
        if !self.ferramentas.is_empty() {
            return;
        }
        match api::ferramentas() {
            Ok(lista) => {
                self.ferramentas = lista.into_iter().map(|f| Escolhida { usar: f.instalada, separada: false, ferramenta: f }).collect();
            }
            Err(e) => self.erro = Some(e),
        }
    }

    fn contas(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        tema::cabecalho(
            ui,
            "Contas de IA",
            "Passo 2 de 3 · Escolha as ferramentas deste perfil. Com \"Só deste perfil\", o login fica separado das outras contas: você entra na primeira vez que abrir o agente.",
        );
        ui.add_space(16.0);
        for escolhida in &mut self.ferramentas {
            let f = &escolhida.ferramenta;
            egui::Frame::new().fill(p.superficie).corner_radius(10).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add_enabled(f.instalada, egui::Checkbox::without_text(&mut escolhida.usar));
                    ui.vertical(|ui| {
                        ui.label(texto_forte(&f.nome, 14.0).color(if f.instalada { p.texto } else { p.suave }));
                        let detalhe = if !f.instalada {
                            "não encontrado no sistema".to_string()
                        } else if f.versao.is_empty() {
                            "instalado".to_string()
                        } else {
                            f.versao.clone()
                        };
                        ui.label(RichText::new(detalhe).color(p.suave).size(12.0));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !(f.instalada && escolhida.usar) {
                            return;
                        }
                        if f.conta_separada {
                            let atual = usize::from(escolhida.separada);
                            if let Some(i) = tema::segmentado(ui, &["Conta do sistema", "Só deste perfil"], atual) {
                                escolhida.separada = i == 1;
                            }
                        } else {
                            ui.label(RichText::new("usa a conta do sistema").color(p.suave).size(12.0));
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if tema::botao_secundario(ui, "Voltar").clicked() {
                self.passo = Passo::Nome;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if tema::botao_principal(ui, "Continuar", true).clicked() {
                    if self.workspace.is_empty() {
                        self.workspace = "Meus projetos".into();
                    }
                    self.passo = Passo::Projeto;
                }
            });
        });
    }

    fn projeto(&mut self, ui: &mut egui::Ui) -> Option<api::Perfil> {
        tema::cabecalho(ui, "Primeiro projeto", "Passo 3 de 3 · Aponte para a pasta de um repositório git. Dá para pular e adicionar depois.");
        ui.add_space(18.0);
        tema::campo(ui, "Workspace", &mut self.workspace, "Ex.: Empresa X, Curso de Rust");
        ui.add_space(12.0);
        if let Some(pasta) = self.seletor.resultado() {
            self.pasta = pasta.display().to_string();
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 150.0);
                tema::campo(ui, "Pasta do projeto", &mut self.pasta, "/home/voce/projetos/meu-projeto");
            });
            ui.vertical(|ui| {
                ui.add_space(20.0);
                let rotulo = if self.seletor.aberto() { "Escolhendo…" } else { "Escolher pasta…" };
                if tema::botao_secundario(ui, rotulo).clicked() && !self.seletor.aberto() {
                    self.seletor.abrir();
                }
            });
        });
        ui.add_space(12.0);
        let sugestao = std::path::Path::new(self.pasta.trim()).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        tema::campo(ui, "Nome do projeto", &mut self.projeto, if sugestao.is_empty() { "Ex.: loja-web" } else { &sugestao });
        ui.add_space(22.0);

        let mut resultado = None;
        ui.horizontal(|ui| {
            if tema::botao_secundario(ui, "Voltar").clicked() {
                self.passo = Passo::Contas;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tem_pasta = !self.pasta.trim().is_empty();
                if tema::botao_principal(ui, "Criar perfil", true).clicked() {
                    resultado = self.concluir(tem_pasta, &sugestao);
                }
                if tema::botao_secundario(ui, "Pular projeto").clicked() {
                    resultado = self.concluir(false, &sugestao);
                }
            });
        });
        resultado
    }

    /// Cria o perfil, grava as contas e, se pedido, o workspace e o projeto.
    /// Se o projeto falhar, o perfil já criado é mantido e dá para tentar de novo.
    fn concluir(&mut self, com_projeto: bool, sugestao: &str) -> Option<api::Perfil> {
        self.erro = None;
        let perfil = match &self.criado {
            Some(p) => p.clone(),
            None => match api::criar_perfil(self.nome.trim(), self.tema.chave()) {
                Ok(p) => {
                    self.criado = Some(p.clone());
                    p
                }
                Err(e) => {
                    self.erro = Some(e);
                    return None;
                }
            },
        };
        let contas: Vec<api::Conta> = self
            .ferramentas
            .iter()
            .filter(|e| e.usar && e.ferramenta.instalada)
            .map(|e| api::Conta { ferramenta: e.ferramenta.id.clone(), modo: if e.separada { "separada" } else { "sistema" }.into() })
            .collect();
        if let Err(e) = api::definir_contas(perfil.id, &contas) {
            self.erro = Some(e);
            return None;
        }
        if com_projeto {
            let workspace = match self.workspace_criado {
                Some(id) => id,
                None => match api::criar_workspace(perfil.id, self.workspace.trim()) {
                    Ok(w) => {
                        self.workspace_criado = Some(w.id);
                        w.id
                    }
                    Err(e) => {
                        self.erro = Some(format!("Workspace: {e}"));
                        return None;
                    }
                },
            };
            let nome = if self.projeto.trim().is_empty() { sugestao.to_string() } else { self.projeto.trim().to_string() };
            if let Err(e) = api::criar_projeto(workspace, &nome, self.pasta.trim()) {
                self.erro = Some(format!("Projeto: {e}"));
                return None;
            }
        }
        Some(perfil)
    }
}
