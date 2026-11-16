//! Página da sprint: o que contar na reunião, por assunto. Cada assunto é
//! uma seção que recolhe (o que fica recolhido é lembrado só enquanto a
//! Colmeia está aberta); "Sem assunto" vem no fim. Na sprint fixa, a tarefa
//! muda de assunto arrastada até a seção (mesmo recolhida), e o lápis da
//! linha edita o título e a descrição só nesta sprint: a tarefa, o quadro e
//! a daily continuam como estão. Fora da sprint fixa (7 dias, este mês), as
//! seções são os projetos e nada se arrasta.
//!
//! "Gerar com IA" (veja gerar.rs) pede ao Claude Code uma proposta para a
//! sprint inteira; ela aparece no lugar das seções até aceitar ou descartar.

use std::collections::HashSet;

use eframe::egui::{self, CornerRadius, Rect, RichText, Sense, Stroke, pos2, vec2};

use super::comum::{self, desenhar_miniatura};
use super::daily::{cabecalho, sozinha, tiradas};
use super::lista::{self, Arrasto, campo_nome};
use super::{Acao, EdicaoTexto, Escopo, Periodo, Registro, leitor};
use crate::api;
use crate::tema::{self, Icone, cores};

/// Uma seção da página: um assunto, "Sem assunto" ou um projeto.
pub(super) struct Secao<'a> {
    /// Para lembrar se está recolhida: -id do assunto, 0 "Sem assunto",
    /// o id do projeto fora da sprint fixa.
    pub chave: i64,
    /// O assunto que recebe o arrasto (0 tira do assunto); `None` num projeto.
    pub assunto: Option<i64>,
    pub nome: String,
    pub slides: Vec<&'a api::Slide>,
}

/// As seções, na ordem: os assuntos da sprint fixa (os vazios também, para
/// receber o arrasto) e "Sem assunto"; fora dela, os projetos do deck.
pub(super) fn secoes(deck: &api::Deck) -> Vec<Secao<'_>> {
    if deck.sprint_id == 0 {
        return comum::secoes_do_deck(deck)
            .into_iter()
            .map(|s| Secao { chave: s.id.max(1), assunto: None, nome: s.projeto.to_string(), slides: s.slides })
            .collect();
    }
    let mut lista: Vec<Secao> = deck
        .assuntos
        .iter()
        .map(|a| Secao { chave: -a.id, assunto: Some(a.id), nome: a.nome.clone(), slides: deck.slides.iter().filter(|s| s.assunto_id == a.id).collect() })
        .collect();
    lista.push(Secao { chave: 0, assunto: Some(0), nome: "Sem assunto".into(), slides: deck.slides.iter().filter(|s| s.assunto_id == 0).collect() });
    lista
}

/// As tarefas soltas numa seção: (assunto, tarefas, nome de um assunto novo).
type Soltas = (i64, Vec<i64>, Option<String>);

const PERIODOS: [(Periodo, &str); 5] =
    [(Periodo::Sprint, "sprint"), (Periodo::Dias7, "7 dias"), (Periodo::Dias14, "14 dias"), (Periodo::Mes, "este mês"), (Periodo::Escolher, "escolher datas")];

impl Registro {
    pub(super) fn pagina_sprint(&mut self, ui: &mut egui::Ui, escopo: &Escopo, escrevendo: &HashSet<i64>, acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let deck = self.deck_sprint.clone();
        match &deck {
            Some(d) => cabecalho(ui, &d.titulo, &d.periodo, escopo, acoes),
            None => cabecalho(ui, "Sprint", &escopo.nome, escopo, acoes),
        }
        ui.add_space(10.0);
        if !(self.periodo == Periodo::Sprint && self.datas_em_edicao(ui)) {
            ui.horizontal(|ui| {
                ui.set_min_height(32.0);
                if self.periodo == Periodo::Sprint {
                    self.setas_da_sprint(ui);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.botao_gerar(ui, deck.as_ref());
                    ui.add_space(8.0);
                    self.chip_periodo(ui);
                });
            });
        }
        if self.periodo == Periodo::Escolher {
            ui.add_space(10.0);
            self.datas_escolhidas(ui);
        }
        ui.add_space(10.0);
        sozinha(ui, "Cada tarefa ganha a frase do agente quando vai para Revisão ou Concluído. Arraste uma tarefa até um assunto para agrupar.");
        ui.add_space(14.0);
        if let Some(d) = deck.as_ref() {
            self.painel_gerar(ui, escopo, d);
        }
        if let Some(e) = self.erro_sprint.clone() {
            ui.label(RichText::new(e).color(p.erro).size(13.5));
            ui.add_space(12.0);
        }
        let Some(deck) = deck else {
            if self.erro_sprint.is_none() && (self.periodo != Periodo::Escolher || self.periodo_atual().is_some()) {
                comum::esqueleto_pagina(ui);
            }
            return;
        };
        // A proposta gerada com IA toma a página inteira enquanto está aberta.
        let mut proposta = false;
        if self.gerar.is_some() {
            egui::ScrollArea::vertical().id_salt("pagina-sprint-proposta").auto_shrink(false).show(ui, |ui| {
                ui.set_max_width(ui.available_width() - comum::MARGEM_ROLAGEM);
                proposta = self.proposta_lado_a_lado(ui, &deck);
            });
        }
        if proposta {
            return;
        }
        if deck.slides.is_empty() && deck.capa.novas.is_empty() {
            self.sprint_vazia(ui, &deck, escopo, acoes);
            return;
        }
        let fixa = deck.sprint_id != 0;
        let secoes = secoes(&deck);
        let mut leitor = std::mem::take(&mut self.leitor_sprint);
        if let Some(t) = self.rolar_ate.take() {
            leitor.escolhida = Some(t);
        }
        if leitor.escolhida.is_none_or(|id| !deck.slides.iter().any(|s| s.tarefa_id == id)) {
            leitor.escolhida = deck.slides.iter().find(|s| !s.apresentada && !s.removida).or(deck.slides.first()).map(|s| s.tarefa_id);
        }
        let area = ui.available_rect_before_wrap();
        let esquerda = Rect::from_min_size(area.min, vec2(leitor::PAUTA, area.height()));
        let direita = Rect::from_min_max(pos2(esquerda.right() + 16.0, area.top()), area.max);
        let op = leitor::OpcoesPauta {
            compacta: false,
            arrastavel: self.conectado,
            assuntos: fixa,
            daily: false,
            sprint_fixa: fixa,
            conectado: self.conectado,
            recorte: escopo.recorte,
        };
        let mut pedidos = Vec::new();
        let mut soltas = None;
        let mut lista = ui.new_child(egui::UiBuilder::new().max_rect(esquerda).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().id_salt("pauta-sprint").auto_shrink(false).show(&mut lista, |ui| {
            ui.set_max_width(leitor::PAUTA - comum::MARGEM_ROLAGEM);
            pedidos = self.pauta(ui, &deck, &secoes, &mut leitor, &op);
            if fixa {
                ui.add_space(4.0);
                soltas = self.novo_assunto(ui, &deck);
                if let Some(t) = tiradas(ui, &deck.fora, escopo.varios(), self.conectado, false) {
                    self.tirar_da_sprint(&ctx, deck.sprint_id, t, false);
                }
            }
            if deck.mais > 0 {
                ui.label(RichText::new(format!("E mais {} tarefas que não cabem na apresentação.", deck.mais)).color(p.suave).size(12.5));
            }
            ui.add_space(12.0);
            self.galeria(ui);
            ui.add_space(16.0);
            lista::rolar_no_arrasto(ui);
        });
        lista::rotulo_do_arrasto(&ctx);
        if let Some((assunto, tarefas, novo)) = soltas
            && self.conectado
        {
            let sprint = deck.sprint_id;
            self.mexer_assuntos(&ctx, move || {
                let assunto = match novo {
                    Some(nome) => api::criar_assunto(sprint, &nome.chars().take(80).collect::<String>())?.id,
                    None => assunto,
                };
                api::assunto_das_tarefas(sprint, &tarefas, assunto).map(|_| String::new())
            });
        }
        let mut detalhe = ui.new_child(egui::UiBuilder::new().max_rect(direita).layout(egui::Layout::top_down(egui::Align::Min)));
        if let Some(id) = leitor.escolhida
            && let Some((indice, s)) = deck.slides.iter().enumerate().find(|(_, s)| s.tarefa_id == id)
        {
            if self.edicao.as_ref().is_some_and(|e| e.tarefa == id && !e.daily) {
                self.linha_em_edicao(&mut detalhe, true);
            } else {
                let projeto = if escopo.varios() || fixa { s.projeto.clone() } else { String::new() };
                let mais = self.painel(&mut detalhe, s, &projeto, indice + 1, deck.slides.len(), &mut leitor, 1.0, true, &op);
                pedidos.extend(mais);
            }
        }
        ui.allocate_rect(area, Sense::hover());
        self.leitor_sprint = leitor;
        if let Some(pd) = self.teclado_do_leitor(&ctx, &deck, false, false) {
            pedidos.push(pd);
        }
        self.tratar_pedidos_leitor(&ctx, pedidos, &deck, false, escrevendo, acoes);
    }

    /// O chip do período: a sprint fixa, os últimos dias, o mês ou datas.
    fn chip_periodo(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let valor = PERIODOS.iter().find(|(p, _)| *p == self.periodo).map_or("sprint", |(_, n)| *n);
        let chip = tema::chip(ui, "Período", valor, self.periodo != Periodo::Sprint);
        let mut escolhido = None;
        egui::Popup::menu(&chip).show(|ui| {
            ui.set_min_width(180.0);
            for (periodo, nome) in PERIODOS {
                if tema::opcao_menu(ui, nome, periodo == self.periodo) {
                    escolhido = Some(periodo);
                    ui.close();
                }
            }
        });
        if let Some(periodo) = escolhido.filter(|p| *p != self.periodo) {
            self.periodo = periodo;
            self.sprint = None;
            self.deck_sprint = None;
            self.edicao = None;
            self.fechar_gerar(false);
            self.pedir_sprint(&ctx);
        }
    }

    /// "Escolher datas": os dois campos; pede quando as duas estão completas.
    fn datas_escolhidas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (de, ate) = (self.de.clone(), self.ate.clone());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(180.0);
                tema::campo(ui, "De", &mut self.de, "DD/MM/AAAA");
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_width(180.0);
                tema::campo(ui, "Até", &mut self.ate, "DD/MM/AAAA");
            });
        });
        if (de != self.de || ate != self.ate) && self.de.trim().len() == 10 && self.ate.trim().len() == 10 {
            self.sprint = None;
            self.deck_sprint = None;
            self.pedir_sprint(&ctx);
        }
    }

    fn sprint_vazia(&mut self, ui: &mut egui::Ui, deck: &api::Deck, escopo: &Escopo, acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let periodo = deck.titulo.trim_start_matches("Sprint · ").replace(" a ", " e ");
        let onde = if escopo.recorte == api::Recorte::Perfil { String::new() } else { format!(" em {}", escopo.nome) };
        ui.label(RichText::new(format!("Nada aconteceu{onde} entre {periodo}.")).color(p.suave).size(13.5));
        ui.add_space(12.0);
        let (rotulo, proximo) = match self.periodo {
            Periodo::Sprint | Periodo::Dias7 => (Some("Ver 14 dias"), Periodo::Dias14),
            Periodo::Dias14 => (Some("Ver este mês"), Periodo::Mes),
            _ => (None, self.periodo),
        };
        ui.horizontal(|ui| {
            if let Some(rotulo) = rotulo
                && tema::botao_secundario(ui, rotulo).clicked()
            {
                self.periodo = proximo;
                self.sprint = None;
                self.deck_sprint = None;
                self.pedir_sprint(&ctx);
            }
            if escopo.recorte != api::Recorte::Perfil && tema::botao_secundario(ui, "Ver todos os projetos").clicked() {
                acoes.push(Acao::Recorte(api::Recorte::Perfil));
            }
        });
    }

    /// Uma seção: o cabeçalho (seta, nome, quantas e o lápis do assunto) e as
    /// linhas. A seção inteira, aberta ou recolhida, recebe o arrasto.
    /// A fala do assunto, entre o cabeçalho e as tarefas: o texto escrito
    /// por você ou gerado pela IA, a caixa para escrever, ou o aviso de que
    /// o Claude Code está escrevendo. Sem fala e sem edição, nada aparece.
    pub(super) fn texto_do_assunto(&mut self, ui: &mut egui::Ui, deck: &api::Deck, assunto: i64, recorte: api::Recorte) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let moldura = egui::Frame::new()
            .fill(p.destaque.gamma_multiply(0.07))
            .stroke(Stroke::new(1.0, p.destaque.gamma_multiply(0.3)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 14, right: 14, top: 10, bottom: 10 });
        if self.texto_assunto.as_ref().is_some_and(|e| e.assunto == assunto) {
            let id = egui::Id::new(("texto-assunto", assunto));
            let foco = ctx.memory(|m| m.has_focus(id));
            let enter = foco && ctx.input_mut(|i| self.composicao.tirar_enters(&mut i.events));
            let esc = foco && ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let mut cancelar = false;
            let mut salvar = false;
            let Some(e) = self.texto_assunto.as_mut() else { return };
            moldura.stroke(Stroke::new(1.0, p.destaque.gamma_multiply(0.45))).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                let r = tema::campo_mensagem(ui, &mut e.texto, 2, 200.0, id, e.salvando);
                if std::mem::take(&mut e.focar) {
                    r.request_focus();
                }
                ui.horizontal(|ui| {
                    let dica =
                        if e.salvando { "Salvando…" } else { "A fala deste assunto na reunião · Enter salva · Shift+Enter nova linha · Esc desfaz" };
                    ui.label(RichText::new(dica).color(p.suave).size(11.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        salvar |= tema::botao_principal(ui, "Salvar", !e.salvando).clicked();
                        ui.add_space(4.0);
                        cancelar |= tema::link_com(ui, "Cancelar", !e.salvando).clicked();
                    });
                });
            });
            let salvando = e.salvando;
            if (esc || cancelar) && !salvando {
                self.texto_assunto = None;
                ctx.request_repaint();
            } else if (enter || salvar) && !salvando {
                self.salvar_texto_assunto(&ctx);
            }
            ui.add_space(8.0);
            return;
        }
        if self.gerando_texto == Some(assunto) {
            moldura.show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0).color(p.destaque));
                    ui.label(RichText::new("O Claude Code está escrevendo a fala deste assunto…").color(p.destaque).size(13.5));
                });
            });
            ui.add_space(8.0);
            return;
        }
        let Some(a) = deck.assuntos.iter().find(|a| a.id == assunto).filter(|a| !a.texto.trim().is_empty()) else { return };
        let gerado = a.texto_origem == "ia";
        let mut editar = false;
        let mut gerar = false;
        let mut apagar = false;
        moldura.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            // O texto é pintado na largura da moldura: um galley solto não a alarga.
            let largura = ui.available_width();
            let mut job =
                egui::text::LayoutJob { wrap: egui::text::TextWrapping { max_width: (largura - 26.0).max(120.0), ..Default::default() }, ..Default::default() };
            comum::anexar_nota(&mut job, a.texto.trim(), 14.0, p.texto, Some(22.0), 8.0);
            let galeria = ui.painter().layout_job(job);
            let (r, _) = ui.allocate_exact_size(vec2(largura, galeria.size().y.max(22.0)), Sense::hover());
            tema::desenhar_icone(ui.painter(), pos2(r.left() + 9.0, r.top() + 11.0), if gerado { Icone::Agente } else { Icone::Lapis }, p.destaque);
            ui.painter().galley(pos2(r.left() + 26.0, r.top()), galeria, p.texto);
            ui.horizontal(|ui| {
                ui.add_space(26.0);
                let origem = if gerado {
                    let n = a.texto_tarefas;
                    format!("Gerado pela IA com {n} {} do assunto", if n == 1 { "tarefa" } else { "tarefas" })
                } else {
                    "Escrito por você".to_string()
                };
                ui.label(RichText::new(origem).color(p.suave).size(12.0));
                ui.label(RichText::new("·").color(p.suave).size(12.0));
                editar = tema::link_com(ui, "Editar", self.conectado).clicked();
                if self.claude != Some(false) {
                    gerar = tema::link_com(ui, "Gerar de novo", self.conectado && self.gerando_texto.is_none())
                        .on_hover_text("O Claude Code escreve outra vez, com as tarefas que estão na sprint agora")
                        .clicked();
                }
                apagar = tema::link_com(ui, "Apagar", self.conectado).clicked();
            });
        });
        ui.add_space(8.0);
        if editar {
            self.composicao = Default::default();
            self.texto_assunto = Some(EdicaoTexto { assunto, texto: a.texto.clone(), salvando: false, focar: true });
        } else if gerar {
            self.gerar_texto_assunto(&ctx, assunto, recorte);
        } else if apagar {
            self.texto_assunto = Some(EdicaoTexto { assunto, texto: String::new(), salvando: false, focar: false });
            self.salvar_texto_assunto(&ctx);
        }
    }

    /// "+ Novo assunto" (soltar aqui cria um com o nome da tarefa) e, sem
    /// nenhum assunto, "Repetir os da sprint anterior".
    fn novo_assunto(&mut self, ui: &mut egui::Ui, deck: &api::Deck) -> Option<Soltas> {
        let ctx = ui.ctx().clone();
        let sprint = deck.sprint_id;
        let mut soltas = None;
        let mut repetir = false;
        ui.horizontal(|ui| {
            if let Some((0, nome)) = self.nome_assunto.as_mut() {
                if let Some(salvar) = campo_nome(ui, nome, 0) {
                    self.salvar_nome_assunto(&ctx, sprint, salvar);
                }
                return;
            }
            let arrastando = egui::DragAndDrop::has_payload_of_type::<Arrasto>(&ctx);
            let r = tema::link_com(ui, "+ Novo assunto", self.conectado);
            if arrastando && r.contains_pointer() {
                ui.painter().rect_stroke(r.rect.expand(2.0), CornerRadius::same(8), Stroke::new(1.0, cores().destaque), egui::StrokeKind::Inside);
            }
            if let Some(a) = r.dnd_release_payload::<Arrasto>() {
                soltas = Some((0, a.tarefas.clone(), Some(a.rotulo.clone())));
            } else if r.on_hover_text("Criar um assunto; soltar uma tarefa aqui cria um com o nome dela").clicked() {
                self.nome_assunto = Some((0, String::new()));
            }
            if deck.assuntos.is_empty() {
                ui.add_space(8.0);
                repetir = tema::link_com(ui, "Repetir os da sprint anterior", self.conectado).clicked();
            }
        });
        if repetir {
            self.mexer_assuntos(&ctx, move || {
                api::repetir_assuntos(sprint)
                    .map(|r| if r.assuntos == 0 { "A sprint anterior não tem assuntos novos para trazer".into() } else { String::new() })
            });
        }
        soltas
    }

    /// Fecha o nome do assunto sendo escrito; com `salvar`, cria ou renomeia.
    fn salvar_nome_assunto(&mut self, ctx: &egui::Context, sprint: i64, salvar: bool) {
        let Some((id, nome)) = self.nome_assunto.take() else { return };
        let nome = nome.trim().to_string();
        if salvar && !nome.is_empty() {
            self.mexer_assuntos(ctx, move || {
                if id == 0 { api::criar_assunto(sprint, &nome).map(|_| String::new()) } else { api::renomear_assunto(id, &nome).map(|_| String::new()) }
            });
        }
    }

    /// Todas as capturas do período, recolhidas. O clique abre o visor.
    fn galeria(&mut self, ui: &mut egui::Ui) {
        let Some(sprint) = self.sprint.clone() else { return };
        if sprint.capturas.is_empty() {
            return;
        }
        let ctx = ui.ctx().clone();
        let mut aberta = self.galeria_aberta;
        tema::secao_recolhivel(ui, &format!("Galeria · {}", sprint.capturas.len()), &mut aberta);
        self.galeria_aberta = aberta;
        if !aberta {
            return;
        }
        ui.add_space(8.0);
        let largura = ui.available_width();
        let mut x = 0.0;
        let mut linha: Option<Rect> = None;
        let mut abrir = None;
        for c in &sprint.capturas {
            let w = match self.miniaturas.ver(c.anexo) {
                Some(comum::Miniatura::Pronta(t)) => (96.0 * t.size_vec2().x / t.size_vec2().y.max(1.0)).clamp(64.0, 192.0),
                _ => 128.0,
            };
            if linha.is_none() || x + w > largura {
                let (r, _) = ui.allocate_exact_size(vec2(largura, 96.0), Sense::hover());
                ui.add_space(8.0 - ui.spacing().item_spacing.y);
                linha = Some(r);
                x = 0.0;
            }
            let base = linha.expect("linha da galeria");
            let caixa = Rect::from_min_size(pos2(base.left() + x, base.top()), vec2(w, 96.0));
            let resposta = desenhar_miniatura(ui, &mut self.miniaturas, caixa, c.anexo, tema::RAIO_CONTROLE).on_hover_text(format!("{} · {}", c.texto, c.dia));
            if resposta.clicked() {
                abrir = Some((c.anexo, c.texto.clone(), c.dia.clone(), c.tarefa_id));
            }
            x += w + 8.0;
        }
        if let Some((anexo, texto, dia, tarefa)) = abrir {
            self.abrir_visor(&ctx, anexo, &texto, &dia, tarefa);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn slide(id: i64, assunto: i64) -> api::Slide {
        api::Slide { tarefa_id: id, assunto_id: assunto, ..Default::default() }
    }

    #[test]
    fn secoes_por_assunto_com_sem_assunto_no_fim() {
        let deck = api::Deck {
            sprint_id: 3,
            assuntos: vec![
                api::Assunto { id: 7, nome: "Vendas".into(), ..Default::default() },
                api::Assunto { id: 8, nome: "Vazio".into(), ..Default::default() },
            ],
            slides: vec![slide(1, 7), slide(2, 0), slide(3, 7)],
            ..Default::default()
        };
        let s = secoes(&deck);
        let resumo: Vec<(i64, &str, Vec<i64>)> = s.iter().map(|x| (x.chave, x.nome.as_str(), x.slides.iter().map(|s| s.tarefa_id).collect())).collect();
        assert_eq!(resumo, vec![(-7, "Vendas", vec![1, 3]), (-8, "Vazio", vec![]), (0, "Sem assunto", vec![2])]);
        assert_eq!(s[2].assunto, Some(0));
    }
}
