//! Página da daily: a prévia do deck. Os números do período, os cartões das
//! tarefas agrupados como nos slides (concluídas, em revisão, aguardando
//! você, trabalhando, com erro) e o texto para copiar, recolhido.
//!
//! Com mais de um projeto (um workspace ou o perfil inteiro), os cartões vêm
//! separados por projeto, com uma fileira de saltos no topo e, em cada
//! projeto, "Só este projeto" e "Abrir quadro". As peças dessa separação
//! servem também à sprint.
//!
//! Uma tarefa já apresentada pode sair da daily de hoje pelo menu do cartão
//! ("Tirar desta daily"); ela fica listada no fim, com "Trazer de volta". A
//! sprint e a linha do tempo continuam com ela.

use eframe::egui::{self, Id, RichText, vec2};

use super::comum::{self, SecaoDeck};
use super::{Acao, Escopo, Registro};
use crate::api;
use crate::tema::{self, TipoAviso, cores};

impl Registro {
    pub(super) fn pagina_daily(&mut self, ui: &mut egui::Ui, escopo: &Escopo, acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let deck = self.deck_daily.clone();
        match &deck {
            Some(d) => cabecalho(ui, &d.titulo, &d.periodo, escopo, acoes),
            None => cabecalho(ui, "Daily", &escopo.nome, escopo, acoes),
        }
        ui.add_space(20.0);
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
        let com_tempo = self.com_tempo(&deck);
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
                    let texto = format!("Nenhuma tarefa trabalhada ({desde}). Quando um agente trabalhar numa tarefa, ela aparece aqui como um slide.");
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
            comum::numeros(ui, &deck.capa.numeros, 32.0, true, com_tempo);
            let secoes = comum::secoes_do_deck(&deck);
            let com_titulo = secoes.len() > 1;
            if com_titulo {
                ui.add_space(20.0);
                if let Some(id) = fileira_de_saltos(ui, &secoes) {
                    self.rolar_secao = Some(id);
                }
            }
            ui.add_space(28.0);
            // Sem títulos por projeto, o cartão diz o projeto (no recorte com vários).
            let com_projeto = !com_titulo && escopo.varios();
            for (i, secao) in secoes.iter().enumerate() {
                if com_titulo {
                    if i > 0 {
                        ui.add_space(40.0 - 16.0);
                    }
                    // A fileira mais cheia da seção (a grade é por grupo de estado).
                    let cartoes = comum::grupos().iter().map(|(chave, ..)| secao.slides.iter().filter(|s| s.grupo == *chave).count()).max().unwrap_or(0);
                    titulo_secao(ui, secao, cartoes, &mut self.rolar_secao, self.conectado, acoes);
                    ui.add_space(12.0);
                }
                for (chave, titulo, cor, marca) in comum::grupos() {
                    let slides: Vec<&api::Slide> = secao.slides.iter().copied().filter(|s| s.grupo == chave).collect();
                    if slides.is_empty() {
                        continue;
                    }
                    rotulo_grupo(ui, titulo, slides.len(), cor, marca);
                    ui.add_space(8.0);
                    let clique = comum::grade(ui, &slides, com_projeto, &mut self.miniaturas, &mut self.rolar_ate, &self.pedidos, self.caixa_aberta, true);
                    match clique {
                        Some(comum::CliqueCartao::TirarDaDaily(t)) => self.tirar_da_daily(&ctx, t, deck.chave_nota.clone(), true),
                        Some(c) => acoes.extend(super::acao_do_clique(c, &deck, None)),
                        None => {}
                    }
                    ui.add_space(16.0);
                }
            }
            if deck.slides.is_empty() {
                let novas = deck.capa.novas.join(", ");
                ui.label(RichText::new(format!("Só tarefas novas: {novas}.")).color(p.suave).size(13.5));
            }
            if deck.mais > 0 {
                ui.label(RichText::new(format!("E mais {} tarefas que não cabem na apresentação.", deck.mais)).color(p.suave).size(13.5));
            }
            sem_atividade(ui, escopo, &secoes, deck.mais > 0);
            if let Some(t) = tiradas(ui, &deck.fora, escopo.varios(), self.conectado) {
                self.tirar_da_daily(&ctx, t, deck.chave_nota.clone(), false);
            }
            ui.add_space(12.0);
            self.texto_da_daily(ui, acoes);
            ui.add_space(24.0);
        });
        let area = egui::Rect::from_min_max(egui::pos2(saida.inner_rect.left(), topo), saida.inner_rect.right_bottom());
        comum::sombra_rolagem(ui.painter(), area, saida.state.offset.y);
    }

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
        // Na altura do texto: os números ficam no mesmo y nas duas visões.
        if tema::link_em_linha(ui, &format!("Ver o workspace {nome} inteiro")).clicked() {
            acoes.push(Acao::Recorte(api::Recorte::Workspace(*workspace)));
        }
    });
}

/// A fileira de saltos: um botão por projeto com o total ("loja-web · 4");
/// o clique rola até a seção. Com mais de um workspace, o nome dele vem antes
/// dos projetos dele. A fileira quebra a linha, nunca rola para o lado.
pub(super) fn fileira_de_saltos(ui: &mut egui::Ui, secoes: &[SecaoDeck]) -> Option<i64> {
    let p = cores();
    let mut ir = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        let mut workspace = None;
        for (i, s) in secoes.iter().enumerate() {
            if !s.workspace.is_empty() && workspace != Some(s.workspace) {
                if i > 0 {
                    ui.add_space(16.0 - 8.0);
                }
                workspace = Some(s.workspace);
                let (r, _) = ui.allocate_exact_size(
                    vec2(ui.painter().layout_no_wrap(s.workspace.to_string(), egui::FontId::proportional(12.5), p.suave).size().x, 32.0),
                    egui::Sense::hover(),
                );
                ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, s.workspace, egui::FontId::proportional(12.5), p.suave);
                ui.add_space(4.0 - 8.0);
            }
            let dica = if s.workspace.is_empty() { format!("Ir para {}", s.projeto) } else { format!("Ir para {} ({})", s.projeto, s.workspace) };
            if tema::botao_secundario(ui, &format!("{} · {}", s.projeto, s.slides.len())).on_hover_text(dica).clicked() {
                ir = Some(s.id);
            }
        }
    });
    ir
}

/// O título de uma seção (um projeto): o workspace suave antes (com mais de
/// um no recorte), o nome, quantas tarefas e, à direita, os links.
/// `cartoes`: quantos cartões a fileira mais cheia da seção tem. Com a
/// grade em duas colunas e só um cartão por fileira, a fileira do título vai
/// só até a borda direita da primeira coluna: os links ficam alinhados ao
/// cartão, não soltos na outra ponta da moldura.
pub(super) fn titulo_secao(ui: &mut egui::Ui, secao: &SecaoDeck, cartoes: usize, rolar: &mut Option<i64>, conectado: bool, acoes: &mut Vec<Acao>) {
    let p = cores();
    let n = secao.slides.len();
    let largura = ui.available_width();
    let largura = if comum::colunas_da_grade(largura) == 2 && cartoes < 2 { comum::largura_coluna(largura) } else { largura };
    let resposta = ui.allocate_ui_with_layout(vec2(largura, 32.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(largura);
        ui.set_height(32.0);
        if !secao.workspace.is_empty() {
            ui.label(RichText::new(format!("{} ›", secao.workspace)).color(p.suave).size(17.0));
        }
        ui.label(tema::texto_forte(secao.projeto, 17.0).color(p.texto));
        ui.label(RichText::new(if n == 1 { "· 1 tarefa".to_string() } else { format!("· {n} tarefas") }).color(p.suave).size(13.5));
        if secao.id == 0 {
            return;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tema::link_com(ui, "Abrir quadro", conectado).on_hover_text("O quadro deste projeto").clicked() {
                acoes.push(Acao::AbrirQuadro(secao.id));
            }
            // Na largura mínima, "Só este projeto" sai antes de empurrar o nome.
            if ui.available_width() > 140.0 {
                ui.add_space(16.0 - ui.spacing().item_spacing.x);
                if tema::link_com(ui, "Só este projeto", conectado).on_hover_text("A daily e a sprint só deste projeto").clicked() {
                    acoes.push(Acao::Recorte(api::Recorte::Projeto(secao.id)));
                }
            }
        });
    });
    if *rolar == Some(secao.id) {
        *rolar = None;
        ui.scroll_to_rect(resposta.response.rect.expand2(vec2(0.0, 8.0)), Some(egui::Align::TOP));
    }
}

/// O rodapé suave: os projetos do recorte sem nada no período ("nada no X").
pub(super) fn sem_atividade(ui: &mut egui::Ui, escopo: &Escopo, secoes: &[SecaoDeck], cortado: bool) {
    // Com slides cortados (o limite da apresentação), não dá para dizer quem ficou sem nada.
    if !escopo.varios() || cortado || secoes.iter().any(|s| s.id == 0) {
        return;
    }
    let ids: Vec<i64> = secoes.iter().map(|s| s.id).collect();
    let nomes: Vec<String> = escopo.sem_atividade(&ids).iter().map(|p| p.nome.clone()).collect();
    if nomes.is_empty() || secoes.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(RichText::new(format!("Sem atividade no período: {}.", nomes.join(", "))).color(cores().suave).size(13.5));
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

/// Rótulo de um grupo de cartões: a marca do estado, nome e quantos.
pub(super) fn rotulo_grupo(ui: &mut egui::Ui, titulo: &str, n: usize, cor: egui::Color32, marca: tema::Marca) {
    let p = cores();
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), egui::Sense::hover());
        tema::marca(ui.painter(), r.center(), 4.5, cor, marca);
        ui.label(tema::texto_forte(format!("{titulo} · {n}"), 13.0).color(p.suave));
    });
}
