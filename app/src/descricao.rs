//! A aba Descrição da tarefa: o que o agente escreveu ao terminar (a nota
//! da daily ou da sprint mais recente), com quem escreveu e quando. Só
//! leitura: para mudar, pede-se ao agente. Buscada ao abrir a aba e de novo
//! quando chega uma nota nova dessa tarefa; nada por tempo.

use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, FontId, RichText};

use crate::api;
use crate::registro::em_segundo_plano;
use crate::tema::{self, cores, forte};

/// Um pedaço do texto, do jeito que aparece: título, item de lista, código ou parágrafo.
#[derive(Debug, PartialEq)]
pub enum Bloco<'a> {
    Titulo(&'a str),
    Item(&'a str),
    Codigo(String),
    Paragrafo(String),
}

/// Lê o texto como um markdown simples: "# título", "- item" (ou "* "),
/// blocos de código entre ``` e parágrafos separados por linha em branco.
pub fn blocos(texto: &str) -> Vec<Bloco<'_>> {
    let mut saida = Vec::new();
    let mut paragrafo: Vec<&str> = Vec::new();
    let mut codigo: Option<Vec<&str>> = None;
    let fechar = |p: &mut Vec<&str>, saida: &mut Vec<Bloco>| {
        if !p.is_empty() {
            saida.push(Bloco::Paragrafo(p.join(" ")));
            p.clear();
        }
    };
    for linha in texto.lines() {
        let t = linha.trim();
        if t.starts_with("```") {
            match codigo.take() {
                Some(c) => saida.push(Bloco::Codigo(c.join("\n"))),
                None => {
                    fechar(&mut paragrafo, &mut saida);
                    codigo = Some(Vec::new());
                }
            }
            continue;
        }
        if let Some(c) = codigo.as_mut() {
            c.push(linha);
            continue;
        }
        if t.is_empty() {
            fechar(&mut paragrafo, &mut saida);
        } else if let Some(titulo) = t.strip_prefix('#') {
            fechar(&mut paragrafo, &mut saida);
            saida.push(Bloco::Titulo(titulo.trim_start_matches('#').trim()));
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            fechar(&mut paragrafo, &mut saida);
            saida.push(Bloco::Item(item));
        } else {
            paragrafo.push(t);
        }
    }
    if let Some(c) = codigo {
        saida.push(Bloco::Codigo(c.join("\n")));
    }
    fechar(&mut paragrafo, &mut saida);
    saida
}

enum Estado {
    Carregando,
    Pronta(api::Descricao),
    Erro(String),
}

type Resposta = Result<api::Descricao, String>;

pub struct Descricao {
    tarefa: i64,
    estado: Estado,
    canal: (Sender<Resposta>, Receiver<Resposta>),
}

impl Descricao {
    pub fn nova(ctx: &egui::Context, tarefa: i64) -> Descricao {
        let mut d = Descricao { tarefa, estado: Estado::Carregando, canal: mpsc::channel() };
        d.pedir(ctx);
        d
    }

    /// Busca de novo (abriu a aba, ou chegou uma nota desta tarefa).
    pub fn pedir(&mut self, ctx: &egui::Context) {
        let tarefa = self.tarefa;
        em_segundo_plano(&self.canal.0, ctx, move || api::descricao(tarefa));
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        while let Ok(r) = self.canal.1.try_recv() {
            self.estado = match r {
                Ok(d) => Estado::Pronta(d),
                Err(e) => Estado::Erro(e),
            };
        }
        let area = ui.available_rect_before_wrap();
        // Uma coluna de leitura, centrada.
        let largura = area.width().min(760.0);
        let coluna = egui::Rect::from_min_size(egui::pos2(area.center().x - largura / 2.0, area.top() + 8.0), egui::vec2(largura, area.height() - 8.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(coluna).layout(egui::Layout::top_down(egui::Align::Min)));
        let d = match &self.estado {
            Estado::Carregando => {
                tema::esqueleto(ui.painter(), egui::Rect::from_min_size(coluna.min, egui::vec2(largura * 0.6, 16.0)), tema::RAIO_ETIQUETA);
                return;
            }
            Estado::Erro(e) => {
                ui.label(RichText::new(format!("Não consegui ler a descrição: {e}")).color(p.erro));
                return;
            }
            Estado::Pronta(d) => d,
        };
        if d.texto.trim().is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(tema::texto_forte("Ainda sem descrição", 16.0).color(p.texto));
                ui.add_space(6.0);
                ui.label(RichText::new("O agente escreve aqui ao terminar o trabalho: o que fez, numa frase para a reunião.").color(p.suave).size(13.5));
            });
            return;
        }
        let quem = if d.autor.is_empty() { "Escrita".to_string() } else { format!("Escrita por {}", d.autor) };
        let onde = if d.tipo == "sprint" { "nota da sprint" } else { "nota da daily" };
        ui.label(RichText::new(format!("{quem} · {} · {onde}", d.quando)).color(p.suave).size(12.5));
        ui.add_space(12.0);
        egui::ScrollArea::vertical().id_salt(("descricao", self.tarefa)).auto_shrink(false).show(&mut ui, |ui| {
            ui.set_width(largura - crate::registro::MARGEM_ROLAGEM);
            for bloco in blocos(&d.texto) {
                match bloco {
                    Bloco::Titulo(t) => {
                        ui.add_space(8.0);
                        ui.label(RichText::new(t).font(forte(15.0)).color(p.texto));
                        ui.add_space(4.0);
                    }
                    Bloco::Item(t) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("•").color(p.destaque).size(14.0));
                            ui.add(egui::Label::new(RichText::new(t).size(14.0).color(p.texto)).wrap());
                        });
                    }
                    Bloco::Codigo(c) => {
                        egui::Frame::new()
                            .fill(p.terminal_fundo)
                            .stroke(egui::Stroke::new(1.0, p.borda))
                            .corner_radius(egui::CornerRadius::same(tema::RAIO_CONTROLE))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(c).font(FontId::monospace(12.5)).color(p.terminal_texto));
                            });
                        ui.add_space(6.0);
                    }
                    Bloco::Paragrafo(t) => {
                        ui.add(egui::Label::new(RichText::new(t).size(14.5).color(p.texto)).wrap());
                        ui.add_space(8.0);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn markdown_simples_em_blocos() {
        let texto = "# Resumo\nTela de pedidos\npronta.\n\n- filtro por data\n* total\n```\ncargo test\n```\nFim";
        assert_eq!(
            blocos(texto),
            vec![
                Bloco::Titulo("Resumo"),
                Bloco::Paragrafo("Tela de pedidos pronta.".into()),
                Bloco::Item("filtro por data"),
                Bloco::Item("total"),
                Bloco::Codigo("cargo test".into()),
                Bloco::Paragrafo("Fim".into()),
            ]
        );
        assert_eq!(blocos("Uma frase só."), vec![Bloco::Paragrafo("Uma frase só.".into())]);
    }
}
