//! Os diálogos dos bancos: a conexão (criar, editar, testar), a senha ao
//! conectar, a confirmação de uma alteração e as remoções.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, FontId, Id, Key, Modifiers, RichText, Sense, vec2};

use super::sql;
use crate::api;
use crate::tema::{self, Pilula, cores, forte};

pub const TIPOS: [(&str, &str); 4] = [("postgres", "PostgreSQL"), ("mysql", "MySQL"), ("sqlserver", "SQL Server"), ("sqlite", "SQLite")];
const MODOS_SSL: [(&str, &str); 4] = [("desligado", "Desligado"), ("preferir", "Preferir"), ("exigir", "Exigir"), ("verificar", "Verificar")];

pub fn porta_padrao(tipo: &str) -> u16 {
    match tipo {
        "postgres" => 5432,
        "mysql" => 3306,
        "sqlserver" => 1433,
        _ => 0,
    }
}

fn backdrop() -> egui::Color32 {
    egui::Color32::from_black_alpha(if tema::claro() { 60 } else { 140 })
}

/// Seletor de arquivo nativo numa thread (a tela não trava).
#[derive(Default)]
pub struct SeletorArquivo {
    recebendo: Option<Receiver<Option<PathBuf>>>,
}

impl SeletorArquivo {
    pub fn abrir(&mut self, ctx: &egui::Context, titulo: &'static str) {
        let (envio, recebendo) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = envio.send(rfd::FileDialog::new().set_title(titulo).pick_file());
            // A tela está parada: acorda para mostrar o arquivo escolhido.
            ctx.request_repaint();
        });
        self.recebendo = Some(recebendo);
    }

    pub fn resultado(&mut self) -> Option<PathBuf> {
        let recebido = self.recebendo.as_ref()?.try_recv().ok()?;
        self.recebendo = None;
        recebido
    }
}

/// O diálogo de conexão.
pub struct Dialogo {
    pub editando: Option<api::ConexaoBanco>,
    pub campos: api::CamposConexao,
    pub senha: String,
    pub guardar: bool,
    nome_editado: bool,
    porta_editada: bool,
    mais_opcoes: bool,
    pub testando: bool,
    pub teste: Option<Result<api::Teste, String>>,
    detalhes: bool,
    pub salvando: bool,
    pub erro: Option<String>,
    nova_pasta: Option<String>,
    arquivo: SeletorArquivo,
    ca: SeletorArquivo,
    focar: bool,
    /// Nomes das conexões que já existem (o nome sugerido não repete).
    pub nomes: Vec<String>,
}

pub enum Pedido {
    Continua,
    Fechar,
    Testar,
    Salvar,
    EsquecerSenha(i64),
}

impl Dialogo {
    pub fn novo(tipo: &str) -> Dialogo {
        let campos =
            api::CamposConexao { tipo: tipo.to_string(), porta: porta_padrao(tipo), ssl: "preferir".into(), host: "localhost".into(), ..Default::default() };
        Dialogo {
            editando: None,
            campos,
            senha: String::new(),
            guardar: true,
            nome_editado: false,
            porta_editada: false,
            mais_opcoes: false,
            testando: false,
            teste: None,
            detalhes: false,
            salvando: false,
            erro: None,
            nova_pasta: None,
            arquivo: SeletorArquivo::default(),
            ca: SeletorArquivo::default(),
            focar: true,
            nomes: Vec::new(),
        }
        .com_nome_sugerido()
    }

    fn com_nome_sugerido(mut self) -> Dialogo {
        self.sugerir_nome();
        self
    }

    /// Com os nomes que já existem (para o sugerido não repetir).
    pub fn com_nomes(mut self, nomes: Vec<String>) -> Dialogo {
        self.nomes = nomes;
        self.sugerir_nome();
        self
    }

    pub fn editar(c: &api::ConexaoBanco) -> Dialogo {
        let mut d = Dialogo::novo(&c.tipo);
        d.campos = campos_de(c);
        d.editando = Some(c.clone());
        d.nome_editado = true;
        d.porta_editada = true;
        d.mais_opcoes = !c.pasta.is_empty() || c.ssl == "verificar";
        d
    }

    /// Duplicar: os mesmos campos, nome novo, sem a senha.
    pub fn duplicar(c: &api::ConexaoBanco) -> Dialogo {
        let mut d = Dialogo::novo(&c.tipo);
        d.campos = campos_de(c);
        d.campos.nome = format!("{} (cópia)", c.nome);
        d.nome_editado = true;
        d.porta_editada = true;
        d
    }

    /// O nome sugerido de banco e host, enquanto você não mexe no nome.
    fn sugerir_nome(&mut self) {
        if self.nome_editado {
            return;
        }
        let c = &self.campos;
        let base = if c.tipo == "sqlite" {
            std::path::Path::new(c.arquivo.trim()).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
        } else {
            match (c.banco.trim().is_empty(), c.host.trim().is_empty()) {
                (false, false) => format!("{}@{}", c.banco.trim(), c.host.trim()),
                (true, false) => c.host.trim().to_string(),
                (false, true) => c.banco.trim().to_string(),
                _ => String::new(),
            }
        };
        self.campos.nome = nome_livre(&base, &self.nomes);
    }

    pub fn mostrar(&mut self, ctx: &egui::Context, chaveiro: bool, pastas: &[String]) -> Pedido {
        if let Some(arquivo) = self.arquivo.resultado() {
            self.campos.arquivo = arquivo.display().to_string();
            self.sugerir_nome();
        }
        if let Some(ca) = self.ca.resultado() {
            self.campos.ssl_ca = ca.display().to_string();
        }
        let p = cores();
        let mut pedido = Pedido::Continua;
        let altura_maxima = (ctx.content_rect().height() - 220.0).max(240.0);
        // Preso pelo topo: o resultado do Testar, o aviso da escrita e "Mais
        // opções" crescem para baixo, sem mover o que está sob o cursor.
        let topo = ((ctx.content_rect().height() - altura_maxima - 160.0) / 2.0).clamp(24.0, 120.0);
        let area = egui::Modal::default_area(Id::new("dialogo-conexao")).anchor(egui::Align2::CENTER_TOP, vec2(0.0, topo));
        let modal = egui::Modal::new(Id::new("dialogo-conexao")).area(area).frame(tema::moldura_janela()).backdrop_color(backdrop()).show(ctx, |ui| {
            ui.set_width(472.0);
            let titulo = match &self.editando {
                Some(c) => format!("Editar {}", c.nome),
                None => "Nova conexão".into(),
            };
            tema::cabecalho(ui, &titulo, "");
            ui.add_space(14.0);
            egui::ScrollArea::vertical().max_height(altura_maxima).show(ui, |ui| {
                ui.set_width(472.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                // O tipo não muda ao editar.
                let atual = TIPOS.iter().position(|(t, _)| *t == self.campos.tipo).unwrap_or(0);
                let nomes: Vec<&str> = TIPOS.iter().map(|(_, n)| *n).collect();
                if self.editando.is_none() {
                    if let Some(i) = tema::segmentado_com_largura(ui, &nomes, atual, Some(472.0)) {
                        let tipo = TIPOS[i].0.to_string();
                        if !self.porta_editada {
                            self.campos.porta = porta_padrao(&tipo);
                        }
                        self.campos.tipo = tipo;
                        self.teste = None;
                        self.sugerir_nome();
                    }
                } else {
                    ui.label(RichText::new(format!("Tipo: {}", nomes[atual])).color(p.suave).size(12.5));
                }
                if self.campos.tipo == "sqlserver" {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        tema::etiqueta_ui(ui, "experimental", tema::fonte_etiqueta(), p.suave);
                        ui.label(RichText::new("Testado só por unidade.").color(p.suave).size(12.5));
                    });
                }
                ui.add_space(12.0);
                let id_nome = Id::new("campo-nome-conexao");
                let r = campo_com_id(ui, "Nome", &mut self.campos.nome, "como aparece na árvore", id_nome);
                if std::mem::take(&mut self.focar) {
                    r.request_focus();
                }
                if r.changed() {
                    self.nome_editado = true;
                }
                ui.add_space(12.0);
                if self.campos.tipo == "sqlite" {
                    // Dá para digitar ou colar o caminho (o núcleo confere) ou escolher.
                    ui.label(RichText::new("Arquivo").color(p.suave).size(12.5));
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let antes = self.campos.arquivo.clone();
                        ui.allocate_ui(vec2(472.0 - 100.0, 34.0), |ui| {
                            campo_com_id(ui, "", &mut self.campos.arquivo, "/caminho/para/loja.db", Id::new("arquivo-sqlite"));
                        });
                        if tema::botao_secundario(ui, "Escolher…").clicked() {
                            self.arquivo.abrir(ui.ctx(), "Escolha o arquivo do SQLite");
                        }
                        if antes != self.campos.arquivo {
                            self.teste = None;
                            self.sugerir_nome();
                        }
                    });
                } else {
                    let antes = (self.campos.host.clone(), self.campos.banco.clone());
                    ui.horizontal(|ui| {
                        ui.allocate_ui(vec2(472.0 - 96.0 - 8.0, 60.0), |ui| {
                            ui.vertical(|ui| {
                                tema::campo(ui, "Host", &mut self.campos.host, "localhost");
                            });
                        });
                        ui.allocate_ui(vec2(96.0, 60.0), |ui| {
                            ui.vertical(|ui| {
                                let mut porta = if self.campos.porta == 0 { String::new() } else { self.campos.porta.to_string() };
                                if tema::campo(ui, "Porta", &mut porta, "").changed() {
                                    self.porta_editada = true;
                                    self.campos.porta = porta.trim().parse().unwrap_or(0);
                                }
                            });
                        });
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.allocate_ui(vec2(232.0, 60.0), |ui| {
                            ui.vertical(|ui| {
                                tema::campo(ui, "Usuário", &mut self.campos.usuario, "");
                            });
                        });
                        ui.allocate_ui(vec2(232.0, 60.0), |ui| {
                            ui.vertical(|ui| {
                                // O exemplo diz onde a senha está (ou vai ficar).
                                let dica = match self.editando.as_ref().map(|c| c.senha.as_str()) {
                                    Some("chaveiro") => "guardada no chaveiro",
                                    Some("memoria") => "só nesta sessão",
                                    Some(_) => "nenhuma",
                                    None if chaveiro && self.guardar => "vai para o chaveiro",
                                    None => "só até fechar a Colmeia",
                                };
                                // O rótulo na mesma altura do "Usuário", com o "Esquecer senha" à direita.
                                let rotulo = ui.label(RichText::new("Senha").color(p.suave).size(12.5));
                                if let Some(c) = &self.editando
                                    && c.senha != "nenhuma"
                                {
                                    let g = ui.painter().layout_no_wrap("Esquecer senha".into(), FontId::proportional(12.5), p.destaque);
                                    let r = egui::Rect::from_min_size(egui::pos2(ui.max_rect().right() - g.size().x, rotulo.rect.top()), g.size());
                                    ui.painter().galley(r.min, g, p.destaque);
                                    let resposta = ui.interact(r, Id::new("esquecer-senha"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if resposta.clicked() {
                                        pedido = Pedido::EsquecerSenha(c.id);
                                    }
                                }
                                ui.add_space(2.0);
                                campo_senha(ui, &mut self.senha, dica, Id::new("senha-conexao"));
                            });
                        });
                    });
                    ui.add_space(8.0);
                    if chaveiro {
                        tema::caixa_marcar(ui, "Guardar no chaveiro", &mut self.guardar);
                    } else {
                        let mut nao = false;
                        tema::caixa_marcar_com(ui, "Guardar no chaveiro", &mut nao, false);
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new("Chaveiro do sistema indisponível: a senha fica só até fechar a Colmeia.").color(p.suave).size(12.5));
                        });
                    }
                    ui.add_space(12.0);
                    tema::campo(ui, "Banco (opcional)", &mut self.campos.banco, "todos os bancos");
                    if antes != (self.campos.host.clone(), self.campos.banco.clone()) {
                        self.sugerir_nome();
                    }
                }
                ui.add_space(12.0);
                tema::secao_recolhivel(ui, "Mais opções", &mut self.mais_opcoes);
                if self.mais_opcoes {
                    ui.horizontal(|ui| {
                        ui.add_space(28.0);
                        ui.vertical(|ui| {
                            let valor = if self.campos.pasta.is_empty() { "nenhuma" } else { self.campos.pasta.as_str() };
                            let resposta = tema::chip(ui, "Pasta", valor, false);
                            egui::Popup::menu(&resposta).show(|ui| {
                                ui.set_min_width(220.0);
                                if tema::opcao_menu(ui, "Nenhuma", self.campos.pasta.is_empty()) {
                                    self.campos.pasta.clear();
                                    ui.close();
                                }
                                for pasta in pastas {
                                    if tema::opcao_menu(ui, pasta, self.campos.pasta == *pasta) {
                                        self.campos.pasta = pasta.clone();
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if tema::opcao_menu(ui, "Nova pasta…", false) {
                                    self.nova_pasta = Some(String::new());
                                    ui.close();
                                }
                            });
                            if let Some(nova) = &mut self.nova_pasta {
                                ui.add_space(6.0);
                                let r = tema::campo(ui, "Nova pasta", nova, "Trabalho");
                                if r.lost_focus() || r.changed() {
                                    self.campos.pasta = nova.trim().to_string();
                                }
                            }
                            if self.campos.tipo != "sqlite" {
                                ui.add_space(10.0);
                                ui.label(RichText::new("SSL/TLS").color(p.suave).size(12.5));
                                let atual = MODOS_SSL.iter().position(|(m, _)| *m == self.campos.ssl).unwrap_or(1);
                                let nomes: Vec<&str> = MODOS_SSL.iter().map(|(_, n)| *n).collect();
                                if let Some(i) = tema::segmentado(ui, &nomes, atual) {
                                    self.campos.ssl = MODOS_SSL[i].0.into();
                                }
                                if self.campos.ssl == "verificar" {
                                    ui.add_space(8.0);
                                    ui.label(RichText::new("Certificado da CA (vazio: as do sistema)").color(p.suave).size(12.5));
                                    ui.horizontal(|ui| {
                                        let largura = 444.0 - 100.0;
                                        let (r, _) = ui.allocate_exact_size(vec2(largura, 32.0), Sense::hover());
                                        ui.painter().rect(
                                            r,
                                            egui::CornerRadius::same(tema::RAIO_CONTROLE),
                                            tema::fundo_campo(p, true),
                                            egui::Stroke::new(1.0, p.borda),
                                            egui::StrokeKind::Inside,
                                        );
                                        let texto = if self.campos.ssl_ca.is_empty() { "as do sistema" } else { self.campos.ssl_ca.as_str() };
                                        tema::texto_sem_inicio(
                                            ui.painter(),
                                            r.min + vec2(10.0, 8.0),
                                            texto,
                                            FontId::proportional(13.5),
                                            p.texto,
                                            largura - 20.0,
                                        );
                                        if tema::botao_secundario(ui, "Escolher…").clicked() {
                                            self.ca.abrir(ui.ctx(), "Escolha o certificado da CA (PEM)");
                                        }
                                    });
                                }
                            }
                        });
                    });
                }
                ui.add_space(16.0);
                ui.label(tema::texto_forte("Permissões", 13.5).color(p.texto));
                ui.add_space(4.0);
                tema::caixa_marcar(ui, "Permitir alterações (INSERT, UPDATE, DDL…)", &mut self.campos.escrita);
                if self.campos.escrita {
                    // Na largura do diálogo (quebra a linha em vez de alargar a caixa).
                    ui.horizontal_top(|ui| {
                        ui.set_max_width(472.0);
                        ui.add_space(4.0);
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 3.5, p.alerta);
                        ui.add(
                            egui::Label::new(
                                RichText::new("Cada alteração pede confirmação. Prefira um usuário de banco só de leitura.").color(p.texto).size(12.5),
                            )
                            .wrap(),
                        );
                    });
                }
                tema::caixa_marcar(ui, "Agentes podem pedir consultas (você aprova cada uma)", &mut self.campos.agentes);
                // O resultado do Testar, numa linha sempre reservada.
                ui.add_space(12.0);
                let sqlite = self.campos.tipo == "sqlite";
                ui.scope(|ui| {
                    ui.set_min_height(28.0);
                    if self.testando {
                        ui.label(RichText::new("Testando…").color(p.suave).size(13.0));
                    } else if let Some(t) = &self.teste {
                        linha_do_teste(ui, t, &mut self.detalhes, sqlite);
                    }
                });
                if let Some(e) = &self.erro {
                    ui.add_space(10.0);
                    ui.label(RichText::new(e).color(p.erro).size(12.5));
                }
            });
            ui.add_space(20.0);
            let valido = !self.campos.nome.trim().is_empty()
                && (if self.campos.tipo == "sqlite" { !self.campos.arquivo.is_empty() } else { !self.campos.host.trim().is_empty() });
            ui.horizontal(|ui| {
                if tema::botao_secundario(ui, "Cancelar").clicked() {
                    pedido = Pedido::Fechar;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_principal(ui, if self.salvando { "Salvando…" } else { "Salvar" }, valido && !self.salvando).clicked() {
                        pedido = Pedido::Salvar;
                    }
                    ui.add_space(8.0);
                    if tema::botao_secundario_com(ui, "Testar", valido && !self.testando).clicked() {
                        pedido = Pedido::Testar;
                    }
                });
            });
        });
        if matches!(pedido, Pedido::Continua) && modal.should_close() {
            return Pedido::Fechar;
        }
        pedido
    }
}

/// O nome, com " 2", " 3"… quando já existe uma conexão com ele.
fn nome_livre(base: &str, nomes: &[String]) -> String {
    let existe = |n: &str| nomes.iter().any(|x| x.eq_ignore_ascii_case(n));
    if base.is_empty() || !existe(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| !existe(n)).unwrap_or_default()
}

fn campos_de(c: &api::ConexaoBanco) -> api::CamposConexao {
    api::CamposConexao {
        pasta: c.pasta.clone(),
        nome: c.nome.clone(),
        tipo: c.tipo.clone(),
        host: c.host.clone(),
        porta: c.porta,
        usuario: c.usuario.clone(),
        banco: c.banco.clone(),
        arquivo: c.arquivo.clone(),
        ssl: if c.ssl.is_empty() { "preferir".into() } else { c.ssl.clone() },
        ssl_ca: c.ssl_ca.clone(),
        escrita: c.escrita,
        agentes: c.agentes,
    }
}

/// O campo do `tema::campo`, com id (para pedir o foco).
fn campo_com_id(ui: &mut egui::Ui, rotulo: &str, texto: &mut String, dica: &str, id: Id) -> egui::Response {
    let p = cores();
    if !rotulo.is_empty() {
        ui.label(RichText::new(rotulo).color(p.suave).size(12.5));
        ui.add_space(2.0);
    }
    let largura = ui.available_width();
    egui::Frame::new()
        .fill(tema::fundo_campo(p, false))
        .stroke(egui::Stroke::new(1.0, p.borda))
        .corner_radius(egui::CornerRadius::same(tema::RAIO_CONTROLE))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(texto)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .desired_width(largura - 22.0)
                    .font(FontId::proportional(14.0))
                    .hint_text(RichText::new(dica).color(p.suave)),
            )
        })
        .inner
}

/// Campo de senha: nunca vem preenchido; o exemplo diz onde a senha está.
pub fn campo_senha(ui: &mut egui::Ui, senha: &mut String, dica: &str, id: Id) -> egui::Response {
    let p = cores();
    let largura = ui.available_width();
    egui::Frame::new()
        .fill(tema::fundo_campo(p, false))
        .stroke(egui::Stroke::new(1.0, p.borda))
        .corner_radius(egui::CornerRadius::same(tema::RAIO_CONTROLE))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(senha)
                    .id(id)
                    .password(true)
                    .frame(egui::Frame::NONE)
                    .desired_width(largura - 22.0)
                    .font(FontId::proportional(14.0))
                    .hint_text(RichText::new(dica).color(p.suave)),
            )
        })
        .inner
}

/// "Conectou em 42 ms · PostgreSQL 16.4 · com TLS", ou o erro em frase simples.
fn linha_do_teste(ui: &mut egui::Ui, teste: &Result<api::Teste, String>, detalhes: &mut bool, sqlite: bool) {
    let p = cores();
    let (ok, cor, texto, detalhe) = match teste {
        Ok(t) if t.ok => {
            // SQLite é um arquivo local: TLS não se aplica.
            let cor = if t.tls || sqlite { p.ok } else { p.alerta };
            let mut texto = format!("Conectou em {} ms", t.ms);
            if !t.servidor.is_empty() {
                texto.push_str(&format!(" · {}", t.servidor));
            }
            (true, cor, texto, String::new())
        }
        Ok(t) => (false, p.erro, t.erro.clone(), t.detalhe.clone()),
        Err(e) => (false, p.erro, e.clone(), String::new()),
    };
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
        if ok {
            ui.painter().circle_filled(r.center(), 3.5, cor);
        } else {
            tema::ponto(ui.painter(), r.center(), 3.5, tema::EstadoVisual::Erro);
        }
        ui.label(RichText::new(&texto).color(p.texto).size(13.0));
        if let Ok(t) = teste
            && t.ok
            && !sqlite
        {
            if t.tls {
                ui.label(RichText::new("· com TLS").color(p.texto).size(13.0));
            } else {
                ui.label(RichText::new("·").color(p.texto).size(13.0));
                ui.label(tema::texto_forte("sem TLS", 13.0).color(p.texto));
            }
        }
    });
    if !detalhe.is_empty() {
        tema::secao_recolhivel(ui, "Detalhes", detalhes);
        if *detalhes {
            let mut d = detalhe;
            tema::campo_multilinha_com(ui, &mut d, 3, 120.0, Id::new("detalhe-teste"), true, FontId::monospace(12.0));
        }
    }
}

/// "Senha de loja-web-dev": pedida ao conectar quando a senha não está à mão.
pub struct PedirSenha {
    pub conexao: i64,
    pub nome: String,
    pub senha: String,
    pub guardar: bool,
    pub enviando: bool,
    pub erro: Option<String>,
    focar: bool,
}

pub enum RespostaSenha {
    Continua,
    Cancelar,
    Conectar,
}

impl PedirSenha {
    pub fn nova(conexao: i64, nome: &str) -> PedirSenha {
        PedirSenha { conexao, nome: nome.to_string(), senha: String::new(), guardar: true, enviando: false, erro: None, focar: true }
    }

    pub fn mostrar(&mut self, ctx: &egui::Context, chaveiro: bool) -> RespostaSenha {
        let p = cores();
        let mut resposta = RespostaSenha::Continua;
        let modal = egui::Modal::new(Id::new("senha-banco")).frame(tema::moldura_janela()).backdrop_color(backdrop()).show(ctx, |ui| {
            ui.set_width(332.0);
            tema::cabecalho(ui, &format!("Senha de {}", self.nome), "");
            ui.add_space(14.0);
            let r = campo_senha(ui, &mut self.senha, "Senha", Id::new("campo-senha-banco"));
            if std::mem::take(&mut self.focar) {
                r.request_focus();
            }
            ui.add_space(8.0);
            if chaveiro {
                tema::caixa_marcar(ui, "Guardar no chaveiro", &mut self.guardar);
            } else {
                // Sem chaveiro (agora): a senha fica só até fechar a Colmeia.
                tema::caixa_marcar(ui, "Lembrar até fechar a Colmeia", &mut self.guardar);
            }
            if let Some(e) = &self.erro {
                ui.add_space(8.0);
                ui.label(RichText::new(e).color(p.erro).size(12.5));
            }
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                if tema::botao_secundario(ui, "Cancelar").clicked() {
                    resposta = RespostaSenha::Cancelar;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_principal(ui, "Conectar", !self.senha.is_empty() && !self.enviando).clicked() {
                        resposta = RespostaSenha::Conectar;
                    }
                });
            });
            // Aqui Enter confirma.
            if !self.senha.is_empty() && !self.enviando && ui.input(|i| i.key_pressed(Key::Enter)) {
                resposta = RespostaSenha::Conectar;
            }
        });
        if matches!(resposta, RespostaSenha::Continua) && modal.should_close() {
            return RespostaSenha::Cancelar;
        }
        resposta
    }
}

/// A confirmação de uma alteração: a instrução inteira, o verbo e o aviso
/// de "sem WHERE". Enter não confirma, Esc cancela e o foco começa em Cancelar.
pub struct ConfirmarEscrita {
    pub conexao: i64,
    pub nome: String,
    pub dialeto: String,
    pub c: api::Confirmacao,
    focar: bool,
}

pub enum RespostaEscrita {
    Continua,
    Cancelar,
    Executar,
}

impl ConfirmarEscrita {
    pub fn nova(conexao: i64, nome: &str, dialeto: &str, c: api::Confirmacao) -> ConfirmarEscrita {
        ConfirmarEscrita { conexao, nome: nome.to_string(), dialeto: dialeto.to_string(), c, focar: true }
    }

    pub fn mostrar(&mut self, ctx: &egui::Context) -> RespostaEscrita {
        let p = cores();
        let mut resposta = RespostaEscrita::Continua;
        // Enter não confirma: tirado da fila antes de qualquer botão ver.
        ctx.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Enter);
            i.consume_key(Modifiers::COMMAND, Key::Enter);
        });
        let modal = egui::Modal::new(Id::new("confirmar-escrita")).frame(tema::moldura_janela()).backdrop_color(backdrop()).show(ctx, |ui| {
            ui.set_width(552.0);
            let banco = if self.c.banco.is_empty() { String::new() } else { format!(" · banco {}", self.c.banco) };
            tema::cabecalho(ui, &format!("Alterar dados em {}{banco}?", self.nome), "");
            ui.add_space(12.0);
            let pilula = Pilula { texto: &self.c.verbo, cor: p.erro, cheio: true, ponto: true, grande: false }.grande();
            let largura = pilula.largura(ui.painter());
            let (r, _) = ui.allocate_exact_size(vec2(largura, 32.0), Sense::hover());
            pilula.pintar(ui.painter(), r.min);
            if self.c.sem_where {
                ui.add_space(10.0);
                egui::Frame::new()
                    .fill(tema::fundo_tingido(p, p.erro, tema::claro()))
                    .corner_radius(egui::CornerRadius::same(tema::RAIO_CONTROLE))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 3.5, p.erro);
                            ui.label(tema::texto_forte("Sem WHERE: altera todas as linhas da tabela.", 13.0).color(p.texto));
                        });
                    });
            }
            ui.add_space(12.0);
            bloco_sql(ui, &self.dialeto, &self.c.sql, 280.0, "sql-escrita");
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                let cancelar = tema::botao_secundario(ui, "Cancelar");
                if std::mem::take(&mut self.focar) {
                    cancelar.request_focus();
                }
                if cancelar.clicked() {
                    resposta = RespostaEscrita::Cancelar;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_alerta(ui, "Executar alteração").clicked() {
                        resposta = RespostaEscrita::Executar;
                    }
                });
            });
        });
        if matches!(resposta, RespostaEscrita::Continua) && modal.should_close() {
            return RespostaEscrita::Cancelar;
        }
        resposta
    }
}

/// O SQL fora do editor (pedido do agente, confirmação): o fundo do editor,
/// o realce e rolagem até a altura dada.
pub fn bloco_sql(ui: &mut egui::Ui, dialeto: &str, texto: &str, altura_maxima: f32, id: &str) {
    bloco_sql_com(ui, dialeto, texto, altura_maxima, id, 0.0, 0.0);
}

/// O bloco de SQL com `direita` px livres à direita (para um botão) e altura
/// mínima `minima`. Devolve o retângulo do bloco.
pub fn bloco_sql_com(ui: &mut egui::Ui, dialeto: &str, texto: &str, altura_maxima: f32, id: &str, direita: f32, minima: f32) -> egui::Rect {
    let p = cores();
    egui::Frame::new()
        .fill(tema::fundo_campo(p, false))
        .stroke(egui::Stroke::new(1.0, p.borda))
        .corner_radius(egui::CornerRadius::same(tema::RAIO_CONTROLE))
        .inner_margin(egui::Margin { left: 10, right: 10 + direita as i8, top: 8, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height((minima - 16.0).max(0.0));
            egui::ScrollArea::both().id_salt(id).max_height(altura_maxima).show(ui, |ui| {
                let job = sql::realce(dialeto, texto, FontId::monospace(12.5), p, None);
                let g = ui.fonts_mut(|f| f.layout_job(job));
                let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                ui.painter().galley(r.min, g, p.texto);
            });
        })
        .response
        .rect
}

/// Confirmação simples (remover a conexão, limpar o histórico), com o botão de alerta.
pub fn confirmar(ctx: &egui::Context, id: &str, titulo: &str, texto: &str, acao: &str, erro: &Option<String>) -> Option<bool> {
    let p = cores();
    let mut resposta = None;
    let modal = egui::Modal::new(Id::new(id)).frame(tema::moldura_janela()).backdrop_color(backdrop()).show(ctx, |ui| {
        ui.set_width(432.0);
        tema::cabecalho(ui, titulo, texto);
        if let Some(e) = erro {
            ui.add_space(10.0);
            ui.label(RichText::new(e).color(p.erro).size(12.5));
        }
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if tema::botao_secundario(ui, "Cancelar").on_hover_text("Esc").clicked() {
                resposta = Some(false);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if tema::botao_alerta(ui, acao).clicked() {
                    resposta = Some(true);
                }
            });
        });
    });
    if resposta.is_none() && modal.should_close() {
        return Some(false);
    }
    resposta
}

/// Os quatro ladrilhos do estado vazio. Devolve o tipo clicado.
pub fn ladrilhos(ui: &mut egui::Ui) -> Option<&'static str> {
    let p = cores();
    let mut escolhido = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (tipo, nome) in TIPOS {
            let (rect, resposta) = ui.allocate_exact_size(vec2(122.0, 88.0), Sense::click());
            let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
            let fundo = if resposta.hovered() { p.realce } else { p.superficie_alta };
            ui.painter().rect(rect, egui::CornerRadius::same(tema::RAIO_CARTAO), fundo, egui::Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
            let experimental = tipo == "sqlserver";
            tema::desenhar_icone(ui.painter(), rect.center_top() + vec2(0.0, 22.0), tema::Icone::Banco, p.suave);
            let y_nome = if experimental { 42.0 } else { 50.0 };
            ui.painter().text(rect.center_top() + vec2(0.0, y_nome), egui::Align2::CENTER_TOP, nome, forte(13.0), p.texto);
            if experimental {
                let largura = ui.painter().layout_no_wrap("experimental".into(), tema::fonte_etiqueta(), p.suave).size().x + 12.0;
                tema::etiqueta(ui.painter(), rect.center_top() + vec2(-largura / 2.0, 62.0), "experimental", tema::fonte_etiqueta(), p.suave);
            }
            if resposta.clicked() {
                escolhido = Some(tipo);
            }
        }
    });
    escolhido
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nome_sugerido_e_porta_padrao() {
        let mut d = Dialogo::novo("postgres");
        d.campos.banco = "loja".into();
        d.campos.host = "db.local".into();
        d.sugerir_nome();
        assert_eq!(d.campos.nome, "loja@db.local");
        d.nome_editado = true;
        d.campos.host = "outro".into();
        d.sugerir_nome();
        assert_eq!(d.campos.nome, "loja@db.local");
        assert_eq!(porta_padrao("mysql"), 3306);
        let c = api::ConexaoBanco { id: 3, nome: "loja".into(), tipo: "mysql".into(), ..Default::default() };
        assert_eq!(Dialogo::duplicar(&c).campos.nome, "loja (cópia)");
        assert!(Dialogo::duplicar(&c).editando.is_none());
        // O sugerido não repete o nome de outra conexão.
        let mut d = Dialogo::novo("postgres").com_nomes(vec!["loja@localhost".into(), "loja@localhost 2".into()]);
        d.campos.banco = "loja".into();
        d.sugerir_nome();
        assert_eq!(d.campos.nome, "loja@localhost 3");
        assert_eq!(Dialogo::novo("sqlite").com_nomes(vec!["loja-web-dev".into()]).campos.nome, "");
    }
}
