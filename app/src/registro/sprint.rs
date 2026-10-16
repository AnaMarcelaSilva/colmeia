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

use eframe::egui::{self, CornerRadius, FontId, Rect, RichText, Sense, Shape, Stroke, pos2, vec2};

use super::comum::{self, desenhar_miniatura};
use super::daily::{cabecalho, sozinha};
use super::frase::{self, Frase};
use super::lista::{self, Arrasto, campo_nome};
use super::{Acao, Escopo, Periodo, Registro};
use crate::api;
use crate::tema::{self, Icone, cores, forte};

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
        let topo = ui.cursor().top();
        let mut rolagem = egui::ScrollArea::vertical().id_salt("pagina-sprint").auto_shrink(false);
        if std::mem::take(&mut self.ao_topo[1]) {
            rolagem = rolagem.vertical_scroll_offset(0.0);
        }
        let saida = rolagem.show(ui, |ui| {
            // A barra de rolagem fica numa faixa só dela, sem nada por baixo.
            ui.set_max_width(ui.available_width() - comum::MARGEM_ROLAGEM);
            if self.proposta_lado_a_lado(ui, &deck) {
                return;
            }
            if deck.slides.is_empty() && deck.capa.novas.is_empty() {
                self.sprint_vazia(ui, &deck, escopo, acoes);
                return;
            }
            let fixa = deck.sprint_id != 0;
            let mut soltas = None;
            for secao in secoes(&deck) {
                if let Some(s) = self.secao(ui, &deck, &secao, escrevendo, escopo.varios(), acoes) {
                    soltas = Some(s);
                }
            }
            if fixa {
                ui.add_space(4.0);
                if let Some(s) = self.novo_assunto(ui, &deck) {
                    soltas = Some(s);
                }
            }
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
            if deck.mais > 0 {
                ui.label(RichText::new(format!("E mais {} tarefas que não cabem na apresentação.", deck.mais)).color(p.suave).size(13.5));
            }
            ui.add_space(12.0);
            self.galeria(ui);
            ui.add_space(24.0);
            lista::rolar_no_arrasto(ui);
        });
        lista::rotulo_do_arrasto(ui.ctx());
        let area = egui::Rect::from_min_max(egui::pos2(saida.inner_rect.left(), topo), saida.inner_rect.right_bottom());
        comum::sombra_rolagem(ui.painter(), area, saida.state.offset.y);
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
    fn secao(
        &mut self,
        ui: &mut egui::Ui,
        deck: &api::Deck,
        secao: &Secao,
        escrevendo: &HashSet<i64>,
        com_projeto: bool,
        acoes: &mut Vec<Acao>,
    ) -> Option<Soltas> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let fixa = deck.sprint_id != 0;
        let arrastando = fixa && egui::DragAndDrop::has_payload_of_type::<Arrasto>(&ctx);
        // "Sem assunto" vazio só aparece como lugar de soltar.
        if secao.slides.is_empty() && secao.assunto == Some(0) && !arrastando {
            return None;
        }
        let id = egui::Id::new(("secao-sprint", secao.chave));
        // O realce de soltar fica por baixo do que vem depois; o retângulo é o do quadro anterior.
        let fundo = ui.painter().add(Shape::Noop);
        let antes: Option<Rect> = ctx.data(|d| d.get_temp(id));
        let sobre = arrastando && secao.assunto.is_some() && antes.zip(ctx.pointer_hover_pos()).is_some_and(|(r, ponto)| r.contains(ponto));
        let aberta = !self.recolhidas.contains(&secao.chave);
        let topo = ui.cursor().top();
        let largura = ui.available_width();

        // O cabeçalho.
        let (rect, cabeca) = ui.allocate_exact_size(vec2(largura, 40.0), Sense::click());
        let renomeando = matches!((self.nome_assunto.as_ref(), secao.assunto), (Some((a, _)), Some(b)) if *a == b && b > 0);
        let cabeca = cabeca.on_hover_cursor(egui::CursorIcon::PointingHand);
        tema::fundo_ao_passar(ui, &cabeca, rect, 10, !sobre);
        let giro = ctx.animate_bool_with_time(id.with("seta"), aberta, tema::TRANSICAO);
        seta(ui.painter(), pos2(rect.left() + 12.0, rect.center().y), giro, p.suave);
        let mut x = rect.left() + 28.0;
        let mut renomear = None;
        let mut apagar = false;
        if renomeando {
            let campo = Rect::from_min_size(pos2(x, rect.center().y - 14.0), vec2(240.0, 28.0));
            let mut pronto = None;
            ui.scope_builder(egui::UiBuilder::new().max_rect(campo), |ui| {
                if let Some((id, nome)) = self.nome_assunto.as_mut() {
                    pronto = campo_nome(ui, nome, *id);
                }
            });
            if let Some(salvar) = pronto {
                self.salvar_nome_assunto(&ctx, deck.sprint_id, salvar);
            }
            x += 248.0;
        } else {
            let cor = if secao.assunto == Some(0) { p.suave } else { p.texto };
            let nome = ui.painter().layout_no_wrap(secao.nome.clone(), forte(15.5), cor);
            let w = nome.size().x;
            ui.painter().galley(pos2(x, rect.center().y - nome.size().y / 2.0), nome, cor);
            x += w + 10.0;
        }
        let n = ui.painter().layout_no_wrap(secao.slides.len().to_string(), forte(11.5), p.suave);
        let pilula = Rect::from_min_size(pos2(x, rect.center().y - 10.0), vec2((n.size().x + 12.0).max(20.0), 20.0));
        ui.painter().rect_filled(pilula, CornerRadius::same(10), p.realce);
        ui.painter().galley(pilula.center() - n.size() / 2.0, n, p.suave);
        x = pilula.right() + 10.0;
        if sobre {
            ui.painter().text(pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, "Soltar aqui", FontId::proportional(12.5), p.destaque);
        }
        // O lápis do assunto, só com o mouse em cima.
        if let Some(assunto) = secao.assunto.filter(|a| *a > 0 && self.conectado && !renomeando) {
            let caixa = Rect::from_min_size(pos2(rect.right() - 32.0, rect.center().y - 13.0), vec2(26.0, 26.0));
            let lapis = ui.interact(caixa, id.with("lapis"), Sense::click());
            let t = tema::transicao(ui, id.with("lapis-aparece"), cabeca.hovered() || lapis.hovered());
            if t > 0.0 {
                if lapis.hovered() {
                    ui.painter().rect_filled(caixa, CornerRadius::same(8), p.realce);
                }
                tema::desenhar_icone(ui.painter(), caixa.center(), Icone::Lapis, (if lapis.hovered() { p.texto } else { p.suave }).gamma_multiply(t));
            }
            if lapis.on_hover_text("Renomear o assunto").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                renomear = Some((assunto, secao.nome.clone()));
            }
            cabeca.context_menu(|ui| {
                ui.set_min_width(180.0);
                if tema::opcao_menu(ui, "Renomear", false) {
                    renomear = Some((assunto, secao.nome.clone()));
                    ui.close();
                }
                if tema::opcao_menu(ui, "Apagar assunto", false) {
                    apagar = true;
                    ui.close();
                }
            });
            if apagar {
                self.mexer_assuntos(&ctx, move || api::remover_assunto(assunto).map(|_| "Assunto apagado; as tarefas ficaram sem assunto".to_string()));
            }
        }
        if cabeca.clicked() && !renomeando && renomear.is_none() {
            self.alternar_secao(secao.chave);
        }
        if renomear.is_some() {
            self.nome_assunto = renomear;
        }

        // As linhas, que aparecem com a transição ao abrir.
        if aberta {
            let t = ctx.animate_bool_with_time(id.with("corpo"), true, tema::TRANSICAO);
            ui.scope(|ui| {
                ui.set_opacity(t);
                egui::Frame::new().inner_margin(egui::Margin { left: 22, ..Default::default() }).show(ui, |ui| {
                    if secao.slides.is_empty() {
                        ui.label(RichText::new("Nenhuma tarefa. Arraste uma para cá.").color(p.suave).size(13.0));
                    }
                    for s in &secao.slides {
                        self.linha_da_sprint(ui, deck, s, escrevendo, com_projeto, acoes);
                    }
                });
            });
        } else {
            ctx.animate_bool_with_time(id.with("corpo"), false, 0.0);
        }
        ui.add_space(8.0);
        let area = Rect::from_min_max(pos2(rect.left() - 6.0, topo - 2.0), pos2(rect.right() + 6.0, ui.cursor().top()));
        ctx.data_mut(|d| d.insert_temp(id, area));
        if sobre {
            ui.painter().set(fundo, frase::realce_de_soltar(area));
        }
        // Soltar: a área inteira da seção recebe.
        let assunto = secao.assunto?;
        let alvo = ui.interact(area, id.with("soltar"), Sense::hover());
        alvo.dnd_release_payload::<Arrasto>().map(|a| (assunto, a.tarefas.clone(), None))
    }

    fn alternar_secao(&mut self, chave: i64) {
        if !self.recolhidas.remove(&chave) {
            self.recolhidas.insert(chave);
        }
    }

    /// Uma tarefa da sprint: a linha, ou o título e a descrição em edição.
    fn linha_da_sprint(&mut self, ui: &mut egui::Ui, deck: &api::Deck, s: &api::Slide, escrevendo: &HashSet<i64>, com_projeto: bool, acoes: &mut Vec<Acao>) {
        let topo = ui.cursor().top();
        if self.edicao.as_ref().is_some_and(|e| e.tarefa == s.tarefa_id && !e.daily) {
            self.linha_em_edicao(ui, true);
            return;
        }
        let fixa = deck.sprint_id != 0;
        let frase = frase::frase_do_slide(s, escrevendo.contains(&s.tarefa_id), false);
        let op = frase::Opcoes {
            projeto: com_projeto.then_some(s.projeto.as_str()),
            editada: self.editadas.contains(&(false, s.tarefa_id)),
            editavel: fixa && self.conectado && !s.removida && !matches!(frase, Frase::Escrevendo),
            arrastavel: fixa && self.conectado && !s.removida,
            daily: false,
        };
        match frase::linha(ui, s, &frase, &op) {
            Some(frase::Clique::Editar) => self.abrir_edicao(s, false, &frase),
            Some(frase::Clique::AbrirTarefa) => acoes.push(Acao::AbrirTarefa { tarefa: s.tarefa_id, agente: None, lousa: false }),
            Some(frase::Clique::Apresentar) => {
                acoes.push(Acao::Apresentar { deck: Box::new(deck.clone()), periodo: self.periodo_atual(), tarefa: Some(s.tarefa_id) });
            }
            Some(frase::Clique::TirarDaDaily) | None => {}
        }
        if self.rolar_ate == Some(s.tarefa_id) {
            self.rolar_ate = None;
            let largura = ui.available_width();
            ui.scroll_to_rect(Rect::from_min_size(pos2(ui.min_rect().left(), topo), vec2(largura, ui.cursor().top() - topo)), Some(egui::Align::Center));
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

/// A seta da seção: para a direita recolhida, para baixo aberta (`giro` de 0 a 1).
fn seta(pintor: &egui::Painter, centro: egui::Pos2, giro: f32, cor: egui::Color32) {
    let angulo = (giro - 1.0) * std::f32::consts::FRAC_PI_2;
    let rot = egui::emath::Rot2::from_angle(angulo);
    let pontos = [vec2(-4.0, -2.0), vec2(4.0, -2.0), vec2(0.0, 2.5)].map(|v| centro + rot * v).to_vec();
    pintor.add(Shape::convex_polygon(pontos, cor, Stroke::NONE));
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
            assuntos: vec![api::Assunto { id: 7, nome: "Vendas".into() }, api::Assunto { id: 8, nome: "Vazio".into() }],
            slides: vec![slide(1, 7), slide(2, 0), slide(3, 7)],
            ..Default::default()
        };
        let s = secoes(&deck);
        let resumo: Vec<(i64, &str, Vec<i64>)> = s.iter().map(|x| (x.chave, x.nome.as_str(), x.slides.iter().map(|s| s.tarefa_id).collect())).collect();
        assert_eq!(resumo, vec![(-7, "Vendas", vec![1, 3]), (-8, "Vazio", vec![]), (0, "Sem assunto", vec![2])]);
        assert_eq!(s[2].assunto, Some(0));
    }
}
