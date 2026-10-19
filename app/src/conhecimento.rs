//! A base de conhecimento do workspace: as notas que todo agente recebe ao
//! começar, as do workspace inteiro e as de cada projeto, e as sugestões dos
//! agentes esperando você. A tela só mostra e pede; quem valida, guarda e
//! decide o que vai ao agente é o núcleo. A lista vem numa thread e volta a
//! ser lida quando um evento diz que a base deste workspace mudou.

use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, CornerRadius, RichText, Stroke, vec2};

use crate::api::{self, Conhecimento};
use crate::registro;
use crate::tema::{self, Icone, cores};

/// O que o agente recebe ao começar, em caracteres (o mesmo limite do núcleo).
const LIMITE_INICIO: usize = 8000;
const MAX_TITULO: usize = 80;
const MAX_TEXTO: usize = 4000;
const LARGURA: f32 = 760.0;

enum Mensagem {
    Lista { workspace: i64, resultado: Result<Vec<Conhecimento>, String> },
    /// Uma mudança pedida ao núcleo terminou; o evento traz a lista nova.
    Feito(Result<(), String>),
}

/// A nota sendo escrita: nova (`id` 0, no `projeto` dado, 0 para o
/// workspace) ou uma existente, sugestão inclusive.
struct Edicao {
    id: i64,
    projeto: i64,
    titulo: String,
    texto: String,
}

/// O que a tela precisa saber do workspace neste quadro.
pub struct Contexto<'a> {
    pub workspace: i64,
    pub nome: &'a str,
    /// Os projetos do workspace: id e nome.
    pub projetos: Vec<(i64, &'a str)>,
    pub automatico: bool,
    /// Muda a cada evento da base de conhecimento.
    pub versao: u64,
    pub pode: bool,
}

pub struct TelaConhecimento {
    workspace: i64,
    notas: Option<Result<Vec<Conhecimento>, String>>,
    versao_lida: Option<u64>,
    edicao: Option<Edicao>,
    remover: Option<i64>,
    erro: Option<String>,
    ocupado: bool,
    envio: Sender<Mensagem>,
    chegando: Receiver<Mensagem>,
}

impl Default for TelaConhecimento {
    fn default() -> Self {
        let (envio, chegando) = mpsc::channel();
        TelaConhecimento { workspace: 0, notas: None, versao_lida: None, edicao: None, remover: None, erro: None, ocupado: false, envio, chegando }
    }
}

impl TelaConhecimento {
    /// Abre a base de outro workspace (a do mesmo fica como estava).
    pub fn abrir(&mut self, workspace: i64) {
        if self.workspace != workspace {
            self.workspace = workspace;
            self.notas = None;
            self.edicao = None;
            self.remover = None;
            self.erro = None;
        }
        self.versao_lida = None;
    }

    fn receber(&mut self) {
        while let Ok(m) = self.chegando.try_recv() {
            match m {
                Mensagem::Lista { workspace, resultado } if workspace == self.workspace => self.notas = Some(resultado),
                Mensagem::Lista { .. } => {}
                Mensagem::Feito(resultado) => {
                    self.ocupado = false;
                    match resultado {
                        Ok(()) => {
                            self.edicao = None;
                            self.remover = None;
                            self.erro = None;
                        }
                        Err(e) => self.erro = Some(e),
                    }
                }
            }
        }
    }

    fn pedir(&mut self, ctx: &egui::Context, pedido: impl FnOnce() -> Result<(), String> + Send + 'static) {
        self.ocupado = true;
        self.erro = None;
        registro::em_segundo_plano(&self.envio, ctx, move || Mensagem::Feito(pedido()));
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        self.receber();
        if self.versao_lida != Some(cx.versao) {
            self.versao_lida = Some(cx.versao);
            let workspace = self.workspace;
            registro::em_segundo_plano(&self.envio, ui.ctx(), move || Mensagem::Lista { workspace, resultado: api::conhecimento(workspace) });
        }
        egui::ScrollArea::vertical().id_salt("conhecimento").auto_shrink([false, false]).show(ui, |ui| {
            let largura = ui.available_width().min(LARGURA);
            let sobra = (ui.available_width() - largura) / 2.0;
            ui.horizontal(|ui| {
                ui.add_space(sobra.max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(largura);
                    ui.add_space(8.0);
                    self.conteudo(ui, cx);
                    ui.add_space(24.0);
                });
            });
        });
    }

    fn conteudo(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        tema::cabecalho(
            ui,
            "Conhecimento",
            &format!("Todo agente que começa em {} já recebe estas notas, sem você repetir. Os agentes também sugerem notas, que só valem quando você aprova.", cx.nome),
        );
        ui.add_space(14.0);
        let notas = match &self.notas {
            None => {
                for _ in 0..3 {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), egui::Sense::hover());
                    tema::esqueleto(ui.painter(), r, 12);
                    ui.add_space(8.0);
                }
                return;
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("Não consegui ler a base de conhecimento: {e}")).color(p.erro));
                return;
            }
            Some(Ok(notas)) => notas.clone(),
        };
        if let Some(e) = &self.erro {
            ui.label(RichText::new(e).color(p.erro).size(13.0));
            ui.add_space(8.0);
        }

        let aprovadas: Vec<&Conhecimento> = notas.iter().filter(|n| !n.sugerida()).collect();
        let sugeridas: Vec<&Conhecimento> = notas.iter().filter(|n| n.sugerida()).collect();
        if aprovadas.is_empty() && sugeridas.is_empty() && self.edicao.is_none() {
            self.vazio(ui, cx);
            self.rodape(ui, cx);
            return;
        }

        if !sugeridas.is_empty() {
            titulo_da_secao(ui, &format!("SUGESTÕES DOS AGENTES · {}", sugeridas.len()), "Só valem depois que você aprova. Leia antes, porque um agente pode ter lido algo errado.", true);
            for n in &sugeridas {
                let alvo = notas.iter().find(|a| a.id == n.substitui).map(|a| a.titulo.as_str());
                self.cartao(ui, n, cx, alvo);
            }
            ui.add_space(12.0);
        }

        let tamanho = |projeto: i64| -> usize {
            aprovadas.iter().filter(|n| n.projeto_id == 0 || n.projeto_id == projeto).map(|n| n.titulo.chars().count() + n.texto.chars().count() + 30).sum()
        };
        let secoes: Vec<(i64, String, String)> = std::iter::once((0, "TODO O WORKSPACE".to_string(), "Vale para todos os projetos daqui.".to_string()))
            .chain(cx.projetos.iter().map(|(id, nome)| {
                let usado = tamanho(*id);
                let medida = if usado > LIMITE_INICIO {
                    format!("Um agente aqui recebe as primeiras notas até {} caracteres; as outras ele lê quando precisar.", milhar(LIMITE_INICIO))
                } else {
                    format!("Um agente aqui recebe {} de {} caracteres.", milhar(usado), milhar(LIMITE_INICIO))
                };
                (*id, nome.to_uppercase(), medida)
            }))
            .collect();
        for (projeto, titulo, explicacao) in secoes {
            let da_secao: Vec<&&Conhecimento> = aprovadas.iter().filter(|n| n.projeto_id == projeto).collect();
            let escrevendo_nova = self.edicao.as_ref().is_some_and(|e| e.id == 0 && e.projeto == projeto);
            if projeto != 0 && da_secao.is_empty() && !escrevendo_nova {
                // Projeto sem notas: uma linha só, para a página não crescer à toa.
                ui.horizontal(|ui| {
                    ui.label(texto_secao(&titulo));
                    ui.label(RichText::new("sem notas próprias").color(p.suave).size(12.0));
                    if cx.pode && self.edicao.is_none() && tema::link_com(ui, "Anotar", true).clicked() {
                        self.edicao = Some(Edicao { id: 0, projeto, titulo: String::new(), texto: String::new() });
                    }
                });
                ui.add_space(10.0);
                continue;
            }
            titulo_da_secao(ui, &titulo, &explicacao, false);
            for n in da_secao {
                self.cartao(ui, n, cx, None);
            }
            if escrevendo_nova {
                self.editor(ui, cx);
            } else if cx.pode && self.edicao.is_none() && tema::link_com(ui, "+ Nova nota", true).clicked() {
                self.edicao = Some(Edicao { id: 0, projeto, titulo: String::new(), texto: String::new() });
            }
            ui.add_space(14.0);
        }
        self.rodape(ui, cx);
    }

    /// Sem nota nenhuma: o que é a base, com exemplos, e o primeiro passo.
    fn vazio(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        egui::Frame::new().fill(p.superficie).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(22)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 22.0, tema::misturar(p.superficie, p.destaque, 0.15));
                tema::desenhar_icone(ui.painter(), r.center(), Icone::Livro, p.destaque);
                ui.add_space(10.0);
                ui.label(tema::texto_forte("Nada anotado ainda", 15.5).color(p.texto));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Escreva uma vez o que você explica para todo agente, como \"as pastas com números são os logs de cada aparelho\", \"o nome do arquivo sai da data e do código da loja\" ou \"sempre olhe também o projeto da API\".",
                    )
                    .color(p.suave)
                    .size(13.0),
                );
                ui.add_space(14.0);
                if cx.pode && tema::botao_principal(ui, "Escrever a primeira nota", true).clicked() {
                    self.edicao = Some(Edicao { id: 0, projeto: 0, titulo: String::new(), texto: String::new() });
                }
            });
        });
        if self.edicao.is_some() {
            ui.add_space(12.0);
            self.editor(ui, cx);
        }
        ui.add_space(14.0);
    }

    /// Uma nota: o título, o texto e as ações. A sugestão diz de onde veio e
    /// o que muda, com aprovar, editar e recusar.
    fn cartao(&mut self, ui: &mut egui::Ui, n: &Conhecimento, cx: &Contexto, muda: Option<&str>) {
        if self.edicao.as_ref().is_some_and(|e| e.id == n.id) {
            self.editor(ui, cx);
            return;
        }
        let p = cores();
        let sugerida = n.sugerida();
        let borda = if sugerida { tema::misturar(p.superficie, p.alerta, 0.55) } else { p.borda };
        egui::Frame::new()
            .fill(p.superficie)
            .stroke(Stroke::new(1.0, borda))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 16, right: 10, top: 10, bottom: 12 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(tema::texto_forte(&n.titulo, 14.0).color(p.texto));
                    if sugerida {
                        let onde = match n.projeto_id {
                            0 => "para todo o workspace".to_string(),
                            id => cx.projetos.iter().find(|(p, _)| *p == id).map_or_else(String::new, |(_, nome)| format!("para {nome}")),
                        };
                        let de = match muda {
                            Some(titulo) => format!("muda a nota \"{titulo}\" {onde}"),
                            None => format!("nota nova {onde}"),
                        };
                        ui.label(RichText::new(de).color(p.suave).size(12.0));
                    }
                    if cx.pode && !sugerida && self.edicao.is_none() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if tema::botao_icone(ui, Icone::Apagar, 26.0).on_hover_text("Apagar a nota").clicked() {
                                self.remover = Some(n.id);
                            }
                            if tema::botao_icone(ui, Icone::Lapis, 26.0).on_hover_text("Editar").clicked() {
                                self.edicao = Some(Edicao { id: n.id, projeto: n.projeto_id, titulo: n.titulo.clone(), texto: n.texto.clone() });
                            }
                        });
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new(&n.texto).color(p.texto).size(13.5));
                if self.remover == Some(n.id) {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Apagar esta nota? Os próximos agentes não vão mais recebê-la.").color(p.texto).size(13.0));
                        if tema::botao_alerta(ui, "Apagar").clicked() && !self.ocupado {
                            let nota = n.id;
                            self.pedir(ui.ctx(), move || api::remover_conhecimento(nota));
                        }
                        if tema::botao_secundario(ui, "Cancelar").clicked() {
                            self.remover = None;
                        }
                    });
                }
                if sugerida && cx.pode {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if tema::botao_principal(ui, "Aprovar", !self.ocupado).clicked() {
                            let nota = n.id;
                            self.pedir(ui.ctx(), move || api::aprovar_conhecimento(nota).map(|_| ()));
                        }
                        if tema::botao_secundario_com(ui, "Editar antes", self.edicao.is_none()).clicked() {
                            self.edicao = Some(Edicao { id: n.id, projeto: n.projeto_id, titulo: n.titulo.clone(), texto: n.texto.clone() });
                        }
                        if tema::botao_secundario_com(ui, "Recusar", !self.ocupado).clicked() {
                            let nota = n.id;
                            self.pedir(ui.ctx(), move || api::remover_conhecimento(nota));
                        }
                    });
                }
            });
        ui.add_space(8.0);
    }

    /// O título e o texto da nota sendo escrita, com salvar e cancelar.
    fn editor(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        let Some(e) = self.edicao.as_mut() else { return };
        let nova = e.id == 0;
        let mut salvar = false;
        let mut cancelar = false;
        egui::Frame::new()
            .fill(p.superficie)
            .stroke(Stroke::new(1.5, p.destaque))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let titulo = tema::campo(ui, "Título", &mut e.titulo, "Ex.: Onde ficam os logs");
                if nova && e.titulo.is_empty() && e.texto.is_empty() {
                    titulo.request_focus();
                }
                ui.add_space(8.0);
                ui.label(RichText::new("O que o agente precisa saber").color(p.suave).size(12.5));
                ui.add_space(2.0);
                tema::campo_multilinha(ui, &mut e.texto, 6, 320.0, ui.id().with(("texto-nota", e.id, e.projeto)), false);
                let (titulo_n, texto_n) = (e.titulo.trim().chars().count(), e.texto.trim().chars().count());
                ui.add_space(4.0);
                let excesso = titulo_n > MAX_TITULO || texto_n > MAX_TEXTO;
                ui.label(RichText::new(format!("{} de {} caracteres", milhar(texto_n), milhar(MAX_TEXTO))).color(if excesso { p.erro } else { p.suave }).size(11.5));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let pronto = titulo_n > 0 && texto_n > 0 && !excesso && !self.ocupado;
                    salvar = tema::botao_principal(ui, "Salvar", pronto).clicked();
                    cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancelar = true;
                }
            });
        ui.add_space(8.0);
        if cancelar {
            self.edicao = None;
            self.erro = None;
        } else if salvar && let Some(e) = &self.edicao {
            let (id, projeto, titulo, texto, workspace) = (e.id, e.projeto, e.titulo.trim().to_string(), e.texto.trim().to_string(), cx.workspace);
            self.pedir(ui.ctx(), move || {
                if id == 0 { api::criar_conhecimento(workspace, projeto, &titulo, &texto) } else { api::editar_conhecimento(id, &titulo, &texto) }.map(|_| ())
            });
        }
    }

    /// A aprovação automática, com o porquê de ela nascer desligada.
    fn rodape(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.borda));
        ui.add_space(12.0);
        let mut automatico = cx.automatico;
        if tema::caixa_marcar_com(ui, "Aceitar na hora o que os agentes anotarem", &mut automatico, cx.pode && !self.ocupado).clicked() && cx.pode {
            let workspace = cx.workspace;
            self.pedir(ui.ctx(), move || api::conhecimento_automatico(workspace, automatico).map(|_| ()));
        }
        ui.label(
            RichText::new("Desligado, a nota de um agente espera você aqui. É mais seguro, porque um agente que leu um arquivo com instruções escondidas não consegue ensiná-las aos próximos.")
                .color(p.suave)
                .size(12.0),
        );
    }
}

fn texto_secao(texto: &str) -> RichText {
    RichText::new(texto).font(tema::forte(10.5)).color(cores().suave)
}

fn titulo_da_secao(ui: &mut egui::Ui, titulo: &str, explicacao: &str, alerta: bool) {
    let p = cores();
    ui.label(texto_secao(titulo).color(if alerta { p.alerta } else { p.suave }));
    ui.label(RichText::new(explicacao).color(p.suave).size(12.0));
    ui.add_space(6.0);
}

/// 8000 vira "8.000".
fn milhar(n: usize) -> String {
    let texto = n.to_string();
    let mut saida = String::new();
    for (i, c) in texto.chars().enumerate() {
        if i > 0 && (texto.len() - i) % 3 == 0 {
            saida.push('.');
        }
        saida.push(c);
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn milhar_com_ponto() {
        assert_eq!(milhar(0), "0");
        assert_eq!(milhar(999), "999");
        assert_eq!(milhar(8000), "8.000");
        assert_eq!(milhar(1234567), "1.234.567");
    }
}
