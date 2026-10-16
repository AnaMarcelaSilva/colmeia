//! Página da daily: o roteiro da fala, numa coluna só. Os grupos são o dia
//! anterior (Ontem), Hoje e Esperando minha resposta; cada tarefa é uma linha
//! com o estado, o título e a frase que o agente deixou quando ela foi para
//! Revisão ou Concluído. Com mais de um projeto no recorte, a linha diz o
//! projeto.
//!
//! Uma tarefa já apresentada pode sair da daily de hoje pelo botão direito
//! ("Tirar desta daily"); ela fica listada no fim, com "Trazer de volta". A
//! sprint e a linha do tempo continuam com ela.

use std::collections::HashSet;

use eframe::egui::{self, Id, RichText, vec2};

use super::frase::{self, Frase};
use super::{Acao, Escopo, Registro, comum};
use crate::api;
use crate::tema::{self, Icone, TipoAviso, cores};

/// Os grupos da daily: o título e as tarefas, na ordem do deck.
fn grupos(deck: &api::Deck) -> Vec<(String, bool, Vec<&api::Slide>)> {
    let (mut antes, mut hoje, mut esperando) = (Vec::new(), Vec::new(), Vec::new());
    let mut nome_antes = None;
    for s in &deck.slides {
        if s.grupo == "aguardando" {
            esperando.push(s);
        } else if s.partes.iter().any(|p| p == "Hoje") || s.partes.is_empty() {
            hoje.push(s);
        } else {
            nome_antes = nome_antes.or_else(|| s.partes.first().cloned());
            antes.push(s);
        }
    }
    [(nome_antes.unwrap_or_else(|| "Ontem".into()), false, antes), ("Hoje".into(), false, hoje), ("Esperando minha resposta".into(), true, esperando)]
        .into_iter()
        .filter(|(_, _, lista)| !lista.is_empty())
        .collect()
}

/// "8 frases · cerca de 1 min de fala": umas 130 palavras por minuto.
pub(super) fn tempo_de_fala(frases: &[&str]) -> String {
    let palavras: usize = frases.iter().map(|f| f.split_whitespace().count()).sum();
    let minutos = palavras.div_ceil(130).max(1);
    let n = frases.len();
    let frases = if n == 1 { "1 frase".to_string() } else { format!("{n} frases") };
    format!("{frases} · cerca de {minutos} min de fala")
}

/// A faixa que explica, uma vez, que a página se escreve sozinha.
pub(super) fn sozinha(ui: &mut egui::Ui, texto: &str) {
    let p = cores();
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::hover());
        ui.painter().rect_filled(r, egui::CornerRadius::same(8), p.destaque.gamma_multiply(0.13));
        tema::desenhar_icone(ui.painter(), r.center(), Icone::Ciclo, p.destaque);
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(texto).color(p.suave).size(12.5)).wrap());
    });
}

impl Registro {
    pub(super) fn pagina_daily(&mut self, ui: &mut egui::Ui, escopo: &Escopo, escrevendo: &HashSet<i64>, acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let deck = self.deck_daily.clone();
        match &deck {
            Some(d) => cabecalho(ui, &d.titulo, &d.periodo, escopo, acoes),
            None => cabecalho(ui, "Daily", &escopo.nome, escopo, acoes),
        }
        ui.add_space(10.0);
        sozinha(ui, "Quando uma tarefa vai para Revisão ou Concluído, o agente deixa uma frase para a reunião. É só ler.");
        ui.add_space(16.0);
        if let Some(e) = self.erro_daily.clone() {
            if comum::faixa_erro(ui, &format!("Não consegui montar a daily: {e}")) {
                self.erro_daily = None;
                self.pedir_daily(&ctx);
            }
            ui.add_space(16.0);
        }
        let Some(deck) = deck else {
            if self.erro_daily.is_none() {
                comum::esqueleto_pagina(ui);
            }
            return;
        };
        let topo = ui.cursor().top();
        let mut rolagem = egui::ScrollArea::vertical().id_salt("pagina-daily").auto_shrink(false);
        if std::mem::take(&mut self.ao_topo[0]) {
            rolagem = rolagem.vertical_scroll_offset(0.0);
        }
        let saida = rolagem.show(ui, |ui| {
            // A barra de rolagem fica numa faixa só dela, sem nada por baixo.
            ui.set_max_width(ui.available_width() - comum::MARGEM_ROLAGEM);
            if deck.slides.is_empty() && deck.capa.novas.is_empty() {
                let desde = deck.periodo.split(" · ").next().unwrap_or_default().to_string();
                if !deck.fora.is_empty() {
                    comum::vazio(ui, "Nada para a daily", "O que houve no período já foi tirado desta daily.", None);
                } else if escopo.recorte == api::Recorte::Perfil {
                    let texto = format!("Nenhuma tarefa trabalhada ({desde}). Quando um agente trabalhar numa tarefa, ela aparece aqui com a frase dela.");
                    comum::vazio(ui, "Nada para a daily", &texto, None);
                } else if comum::vazio(
                    ui,
                    &format!("Nada para a daily em {}", escopo.nome),
                    &format!("Nenhuma tarefa trabalhada ({desde})."),
                    Some("Ver todos os projetos"),
                ) {
                    acoes.push(Acao::Recorte(api::Recorte::Perfil));
                }
                if let Some(t) = tiradas(ui, &deck.fora, escopo.varios(), self.conectado) {
                    self.tirar_da_daily(&ctx, t, deck.chave_nota.clone(), false);
                }
                return;
            }
            let mut faladas = Vec::new();
            for (titulo, espera, slides) in grupos(&deck) {
                cabecalho_grupo(ui, &titulo, espera);
                for s in slides {
                    // A frase de ontem só vale para quem parou; quem segue trabalhando ganha outra.
                    let anterior = !matches!(s.grupo.as_str(), "trabalhando" | "aguardando" | "erros");
                    let frase = frase::frase_do_slide(s, escrevendo.contains(&s.tarefa_id), anterior);
                    if let Frase::Pronta(t) = frase {
                        faladas.push(t);
                    }
                    self.linha_da_daily(ui, &deck, s, &frase, escopo.varios(), acoes);
                }
                ui.add_space(18.0);
            }
            if deck.slides.is_empty() {
                let novas = deck.capa.novas.join(", ");
                ui.label(RichText::new(format!("Só tarefas novas: {novas}.")).color(p.suave).size(13.5));
            }
            if deck.mais > 0 {
                ui.label(RichText::new(format!("E mais {} tarefas que não cabem na apresentação.", deck.mais)).color(p.suave).size(13.5));
            }
            if !faladas.is_empty() {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 18.0), egui::Sense::hover());
                    tema::desenhar_icone(ui.painter(), r.center(), Icone::Relogio, p.suave);
                    ui.label(RichText::new(format!("{} · cada frase vira um slide", tempo_de_fala(&faladas))).color(p.suave).size(12.5));
                });
            }
            if let Some(t) = tiradas(ui, &deck.fora, escopo.varios(), self.conectado) {
                self.tirar_da_daily(&ctx, t, deck.chave_nota.clone(), false);
            }
            ui.add_space(16.0);
            self.texto_da_daily(ui, acoes);
            ui.add_space(24.0);
        });
        let area = egui::Rect::from_min_max(egui::pos2(saida.inner_rect.left(), topo), saida.inner_rect.right_bottom());
        comum::sombra_rolagem(ui.painter(), area, saida.state.offset.y);
    }

    /// Uma tarefa da daily: a linha, ou o campo da frase quando em edição.
    fn linha_da_daily(&mut self, ui: &mut egui::Ui, deck: &api::Deck, s: &api::Slide, frase: &Frase, com_projeto: bool, acoes: &mut Vec<Acao>) {
        let topo = ui.cursor().top();
        if self.edicao.as_ref().is_some_and(|e| e.tarefa == s.tarefa_id && e.daily) {
            self.linha_em_edicao(ui, false);
            return;
        }
        let op = frase::Opcoes {
            projeto: com_projeto.then_some(s.projeto.as_str()),
            editada: self.editadas.contains(&(true, s.tarefa_id)),
            editavel: self.conectado && !s.removida && !matches!(frase, Frase::Escrevendo),
            arrastavel: false,
            daily: self.conectado,
        };
        match frase::linha(ui, s, frase, &op) {
            Some(frase::Clique::Editar) => self.abrir_edicao(s, true, frase),
            Some(frase::Clique::AbrirTarefa) => acoes.push(Acao::AbrirTarefa { tarefa: s.tarefa_id, agente: None, lousa: false }),
            Some(frase::Clique::Apresentar) => acoes.push(Acao::Apresentar { deck: Box::new(deck.clone()), periodo: None, tarefa: Some(s.tarefa_id) }),
            Some(frase::Clique::TirarDaDaily) => self.tirar_da_daily(ui.ctx(), s.tarefa_id, deck.chave_nota.clone(), true),
            None => {}
        }
        // Ao voltar da apresentação, a página mostra a tarefa do último slide visto.
        if self.rolar_ate == Some(s.tarefa_id) {
            self.rolar_ate = None;
            let largura = ui.available_width();
            ui.scroll_to_rect(
                egui::Rect::from_min_size(egui::pos2(ui.min_rect().left(), topo), vec2(largura, ui.cursor().top() - topo)),
                Some(egui::Align::Center),
            );
        }
    }

    /// O texto para copiar, recolhido no fim (Ctrl+Shift+D abre e põe o foco).
    fn texto_da_daily(&mut self, ui: &mut egui::Ui, acoes: &mut Vec<Acao>) {
        let Some(daily) = self.daily.clone() else { return };
        let mut aberta = self.texto_daily_aberto;
        let pode_copiar = !daily.vazio && self.conectado && !self.atualizando;
        ui.horizontal(|ui| {
            tema::secao_recolhivel(ui, "Texto da daily", &mut aberta);
            if aberta {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let copiar = tema::botao_secundario_com(ui, "Copiar", pode_copiar);
                    let copiar = if self.atualizando { copiar.on_hover_text("Atualizando…") } else { copiar };
                    if copiar.clicked() {
                        ui.ctx().copy_text(self.texto_daily.clone());
                        acoes.push(Acao::Avisar(TipoAviso::Neutro, "Copiado".into()));
                    }
                    if self.texto_daily != daily.texto {
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Refazer texto").clicked() {
                            self.texto_daily = daily.texto.clone();
                            self.texto_daily_com_tempo = daily.tempo_agentes && super::tem_tempo(&daily.texto);
                        }
                    }
                });
            }
        });
        self.texto_daily_aberto = aberta;
        if !aberta {
            return;
        }
        ui.add_space(8.0);
        let resposta = tema::campo_multilinha(ui, &mut self.texto_daily, 6, 260.0, Id::new("texto-daily"), daily.vazio);
        // Foco no texto uma vez só (pedir em todo quadro trava os eventos).
        if std::mem::take(&mut self.focar_daily) && !daily.vazio {
            resposta.request_focus();
        }
    }
}

/// O nome do grupo, com um fio embaixo; "Esperando minha resposta" em âmbar.
fn cabecalho_grupo(ui: &mut egui::Ui, titulo: &str, espera: bool) {
    let p = cores();
    ui.label(tema::texto_forte(titulo, 15.0).color(if espera { p.alerta } else { p.texto }));
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top() + 2.0, egui::Stroke::new(1.0, p.borda.gamma_multiply(0.6)));
    ui.add_space(8.0);
}

/// O cabeçalho da página: o título e o período. Na visão de um projeto de um
/// workspace com mais projetos, o link "Ver o workspace X inteiro".
pub(super) fn cabecalho(ui: &mut egui::Ui, titulo: &str, explicacao: &str, escopo: &Escopo, acoes: &mut Vec<Acao>) {
    let p = cores();
    let Some((workspace, nome)) = &escopo.workspace_maior else {
        tema::cabecalho(ui, titulo, explicacao);
        return;
    };
    tema::cabecalho(ui, titulo, "");
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new(explicacao).color(p.suave).size(13.5));
        ui.label(RichText::new("  ·  ").color(p.suave).size(13.5));
        if tema::link_em_linha(ui, &format!("Ver o workspace {nome} inteiro")).clicked() {
            acoes.push(Acao::Recorte(api::Recorte::Workspace(*workspace)));
        }
    });
}

/// As tarefas tiradas da daily de hoje, cada uma com "Trazer de volta".
/// Devolve a que deve voltar.
fn tiradas(ui: &mut egui::Ui, fora: &[api::TarefaFora], com_projeto: bool, conectado: bool) -> Option<i64> {
    if fora.is_empty() {
        return None;
    }
    let p = cores();
    let mut voltar = None;
    ui.add_space(16.0);
    ui.label(RichText::new(format!("Tiradas desta daily · {}", fora.len())).color(p.suave).size(13.5));
    ui.add_space(4.0);
    for t in fora {
        ui.horizontal(|ui| {
            let mut texto = format!("“{}”", t.titulo);
            if com_projeto && !t.projeto.is_empty() {
                texto = format!("{texto} · {}", t.projeto);
            }
            ui.label(RichText::new(texto).color(p.texto).size(13.5));
            let r = tema::link_com(ui, "Trazer de volta", conectado);
            if r.on_hover_text("Volta para a daily de hoje").clicked() {
                voltar = Some(t.tarefa_id);
            }
        });
    }
    voltar
}

#[cfg(test)]
mod testes {
    use super::*;

    fn slide(id: i64, grupo: &str, partes: &[&str]) -> api::Slide {
        api::Slide { tarefa_id: id, grupo: grupo.into(), partes: partes.iter().map(|p| p.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn grupos_da_daily_ontem_hoje_e_esperando() {
        let deck = api::Deck {
            slides: vec![
                slide(1, "concluidas", &["Ontem"]),
                slide(2, "concluidas", &["Ontem", "Hoje"]),
                slide(3, "aguardando", &["Hoje"]),
                slide(4, "trabalhando", &[]),
            ],
            ..Default::default()
        };
        let g = grupos(&deck);
        let ids: Vec<(String, Vec<i64>)> = g.iter().map(|(t, _, l)| (t.clone(), l.iter().map(|s| s.tarefa_id).collect())).collect();
        assert_eq!(ids, vec![("Ontem".into(), vec![1]), ("Hoje".into(), vec![2, 4]), ("Esperando minha resposta".into(), vec![3])]);
        assert!(g[2].1);
        // O dia anterior pode ser outro ("Sexta", depois do fim de semana).
        let deck = api::Deck { slides: vec![slide(1, "concluidas", &["Sexta"])], ..Default::default() };
        assert_eq!(grupos(&deck)[0].0, "Sexta");
    }

    #[test]
    fn tempo_de_fala_pelas_palavras() {
        assert_eq!(tempo_de_fala(&["A busca acha o cliente pelo telefone."]), "1 frase · cerca de 1 min de fala");
        let longa = "palavra ".repeat(200);
        assert_eq!(tempo_de_fala(&[&longa, "outra"]), "2 frases · cerca de 2 min de fala");
    }
}
