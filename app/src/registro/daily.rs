//! Página da daily: um bloco por projeto, na ordem da barra lateral, como
//! a pessoa fala na reunião. Dentro do bloco, uma tarefa por linha com a
//! nota que o agente deixou (ou que você escreveu), em itens quando ela tem
//! itens. O dia (quando não é hoje) e o estado que pede atenção viram selos
//! na linha; as tarefas que esperam resposta são contadas no alto.
//!
//! Uma tarefa já apresentada pode sair da daily de hoje pelo botão direito
//! ("Tirar desta daily"); ela fica listada no fim, com "Trazer de volta". A
//! sprint e a linha do tempo continuam com ela.

use std::collections::HashSet;

use eframe::egui::{self, FontId, Id, Rect, RichText, UiBuilder, pos2, vec2};

use super::{Acao, Escopo, Registro, comum, leitor, lista, sprint};
use crate::api;
use crate::tema::{self, Icone, TipoAviso, cores};

/// A parte do dia que vira selo na linha: a primeira que não é "Hoje",
/// só quando a tarefa não aparece hoje.
fn parte_do_slide(s: &api::Slide) -> Option<&str> {
    if s.partes.iter().any(|p| p == "Hoje") {
        return None;
    }
    s.partes.iter().find(|p| *p != "No período").map(String::as_str)
}

/// O texto para copiar, montado do deck: um bloco por projeto com as notas
/// de cada tarefa (uma linha segue o título; itens vão embaixo, recuados),
/// e no fim as tarefas que esperam resposta, com o projeto entre
/// parênteses. Com um projeto só, sem a linha do nome.
pub(super) fn texto_em_blocos(deck: &api::Deck) -> String {
    let secoes = comum::secoes_do_deck(deck);
    let varios = secoes.len() > 1;
    let mut blocos: Vec<String> = Vec::new();
    let mut esperando: Vec<String> = Vec::new();
    for secao in &secoes {
        let mut linhas = Vec::new();
        for s in &secao.slides {
            if s.grupo == "aguardando" && !s.removida {
                let projeto = if varios { format!(" ({})", secao.projeto) } else { String::new() };
                esperando.push(format!("- {}{projeto}", s.titulo.trim_end_matches('.')));
                continue;
            }
            linhas.push(linha_do_texto(s));
        }
        if linhas.is_empty() {
            continue;
        }
        let corpo = linhas.join("\n");
        blocos.push(if varios { format!("{}\n{corpo}", secao.projeto) } else { corpo });
    }
    if !esperando.is_empty() {
        blocos.push(format!("Esperando minha resposta\n{}", esperando.join("\n")));
    }
    blocos.join("\n\n")
}

/// Uma tarefa no texto: o título, o estado entre parênteses quando conta,
/// e a nota (de hoje ou de ontem).
fn linha_do_texto(s: &api::Slide) -> String {
    let titulo = s.titulo.trim_end_matches('.');
    let anterior = !matches!(s.grupo.as_str(), "trabalhando" | "aguardando" | "erros");
    let nota = if !s.nota.trim().is_empty() {
        Some(s.nota.trim().to_string())
    } else {
        s.nota_anterior.as_ref().filter(|a| anterior && !a.texto.trim().is_empty()).map(|a| a.texto.trim().to_string())
    };
    let mut estados = Vec::new();
    if parte_do_slide(s).is_some() {
        estados.push("ontem");
    }
    match s.grupo.as_str() {
        "trabalhando" => estados.push("em andamento"),
        "revisao" => estados.push("em revisão"),
        "erros" => estados.push("parou com erro"),
        _ => {}
    }
    let estado = if estados.is_empty() { String::new() } else { format!(" ({})", estados.join(", ")) };
    match nota {
        Some(n) if n.contains('\n') || comum::tem_itens(&n) => {
            let itens: Vec<String> = n.lines().map(|l| format!("  {}", l.trim_end())).collect();
            format!("- {titulo}{estado}\n{}", itens.join("\n"))
        }
        Some(n) => format!("- {titulo}{estado}. {n}"),
        None => format!("- {titulo}{estado}"),
    }
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
        // O cabeçalho; à direita, "N de M apresentadas" e "Daily apresentada".
        ui.horizontal(|ui| {
            ui.vertical(|ui| match &deck {
                Some(d) => cabecalho(ui, &d.titulo, &d.periodo, escopo, acoes),
                None => cabecalho(ui, "Daily", &escopo.nome, escopo, acoes),
            });
            if let Some(d) = deck.as_ref().filter(|d| !d.slides.is_empty()) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    ui.add_space(4.0);
                    if d.fechada_em.is_empty() {
                        let b = tema::botao_secundario_com(ui, "Daily apresentada", self.conectado)
                            .on_hover_text("Fecha a daily de hoje: a próxima começa daqui, com o que ficou aberto, e os vistos zeram");
                        if b.clicked() {
                            self.fechar_daily(&ctx, d.chave_nota.clone());
                        }
                    } else {
                        if tema::link_com(ui, "fechar de novo", self.conectado).on_hover_text("A próxima daily começa de agora").clicked() {
                            self.fechar_daily(&ctx, d.chave_nota.clone());
                        }
                        ui.label(RichText::new(format!("Daily apresentada às {}", d.fechada_em)).color(p.ok).size(13.0));
                    }
                    ui.add_space(12.0);
                    leitor::progresso(ui, d.apresentadas, d.slides.iter().filter(|s| !s.removida).count());
                });
            }
        });
        ui.add_space(10.0);
        sozinha(
            ui,
            "A pauta à esquerda, na ordem em que você fala (arraste para mudar). À direita a tarefa inteira, com a frase, os cartões e as imagens. O visto marca o que já foi apresentado.",
        );
        ui.add_space(14.0);
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
        if deck.slides.is_empty() && deck.capa.novas.is_empty() {
            let desde = deck.periodo.split(" · ").next().unwrap_or_default().to_string();
            if !deck.fora.is_empty() {
                comum::vazio(ui, "Nada para a daily", "O que houve no período já foi tirado desta daily.", None);
            } else if !deck.fechada_em.is_empty() {
                comum::vazio(ui, "Daily apresentada", "A próxima daily começa com o que acontecer daqui em diante e com o que ficou aberto.", None);
            } else if escopo.recorte == api::Recorte::Perfil {
                let texto = format!("Nenhuma tarefa trabalhada ({desde}). Quando um agente trabalhar numa tarefa, ela aparece aqui na pauta do projeto dela.");
                comum::vazio(ui, "Nada para a daily", &texto, None);
            } else if comum::vazio(
                ui,
                &format!("Nada para a daily em {}", escopo.nome),
                &format!("Nenhuma tarefa trabalhada ({desde})."),
                Some("Ver todos os projetos"),
            ) {
                acoes.push(Acao::Recorte(api::Recorte::Perfil));
            }
            if let Some(t) = tiradas(ui, &deck.fora, escopo.varios(), self.conectado, true) {
                self.tirar_da_daily(&ctx, t, deck.chave_nota.clone(), false);
            }
            return;
        }

        // A pauta e o painel. A tarefa aberta, de começo, é a primeira ainda
        // não apresentada; ao voltar da tela cheia, a última vista.
        let secoes = sprint::secoes(&deck);
        let mut leitor = std::mem::take(&mut self.leitor_daily);
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
            assuntos: false,
            daily: true,
            sprint_fixa: false,
            conectado: self.conectado,
            recorte: escopo.recorte,
        };
        let mut pedidos = Vec::new();
        let esperando = deck.slides.iter().filter(|s| s.grupo == "aguardando" && !s.removida).count();
        let mut lista = ui.new_child(UiBuilder::new().max_rect(esquerda).layout(egui::Layout::top_down(egui::Align::Min)));
        // Nada da pauta passa por baixo do painel (ele é translúcido).
        lista.set_clip_rect(esquerda.intersect(lista.clip_rect()));
        egui::ScrollArea::vertical().id_salt("pauta-daily").auto_shrink(false).show(&mut lista, |ui| {
            ui.set_max_width(leitor::PAUTA - comum::MARGEM_ROLAGEM);
            if esperando > 0 {
                let texto = if esperando == 1 { "1 tarefa espera minha resposta".to_string() } else { format!("{esperando} tarefas esperam minha resposta") };
                ui.label(tema::texto_forte(texto, 13.0).color(p.alerta));
                ui.add_space(8.0);
            }
            pedidos = self.pauta(ui, &deck, &secoes, &mut leitor, &op);
            if deck.mais > 0 {
                ui.label(RichText::new(format!("E mais {} tarefas que não cabem na apresentação.", deck.mais)).color(p.suave).size(12.5));
            }
            if let Some(t) = tiradas(ui, &deck.fora, escopo.varios(), self.conectado, true) {
                self.tirar_da_daily(&ctx, t, deck.chave_nota.clone(), false);
            }
            ui.add_space(14.0);
            self.texto_da_daily(ui, acoes);
            ui.add_space(16.0);
            lista::rolar_no_arrasto(ui);
        });
        lista::rotulo_do_arrasto(&ctx);
        let mut detalhe = ui.new_child(UiBuilder::new().max_rect(direita).layout(egui::Layout::top_down(egui::Align::Min)));
        if let Some(id) = leitor.escolhida
            && let Some((indice, s)) = deck.slides.iter().enumerate().find(|(_, s)| s.tarefa_id == id)
        {
            if self.edicao.as_ref().is_some_and(|e| e.tarefa == id && e.daily) {
                self.linha_em_edicao(&mut detalhe, true);
            } else {
                let projeto = secoes.iter().find(|sec| sec.slides.iter().any(|x| x.tarefa_id == id)).map(|sec| sec.nome.clone()).unwrap_or_default();
                let mais = self.painel(&mut detalhe, s, &projeto, indice + 1, deck.slides.len(), &mut leitor, 1.0, true, &op);
                pedidos.extend(mais);
            }
        }
        ui.allocate_rect(area, egui::Sense::hover());
        self.leitor_daily = leitor;
        // O teclado mexe no estado já devolvido ao registro.
        if let Some(pd) = self.teclado_do_leitor(&ctx, &deck, true, false) {
            pedidos.push(pd);
        }
        self.tratar_pedidos_leitor(&ctx, pedidos, &deck, true, escrevendo, acoes);
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
                    if self.texto_daily != self.texto_daily_base {
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Refazer texto").clicked() {
                            self.texto_daily = self.texto_daily_base.clone();
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
        if tema::link_em_linha(ui, &format!("Ver o workspace {nome} inteiro")).clicked() {
            acoes.push(Acao::Recorte(api::Recorte::Workspace(*workspace)));
        }
    });
}

/// As tarefas tiradas da daily de hoje, cada uma com "Trazer de volta".
/// Devolve a que deve voltar.
pub(super) fn tiradas(ui: &mut egui::Ui, fora: &[api::TarefaFora], com_projeto: bool, conectado: bool, daily: bool) -> Option<i64> {
    if fora.is_empty() {
        return None;
    }
    let p = cores();
    let mut voltar = None;
    ui.add_space(16.0);
    let onde = if daily { "daily" } else { "sprint" };
    ui.label(RichText::new(format!("Tiradas desta {onde} · {}", fora.len())).color(p.suave).size(13.5));
    ui.add_space(4.0);
    // O título cabe na largura da pauta, cortado com "…" (o inteiro no
    // hover), e o "Trazer de volta" fica sempre à vista no fim da linha.
    let largura_link = ui.painter().layout_no_wrap("Trazer de volta".into(), FontId::proportional(13.5), p.texto).size().x + 16.0;
    for t in fora {
        ui.horizontal(|ui| {
            let mut texto = format!("“{}”", t.titulo);
            if com_projeto && !t.projeto.is_empty() {
                texto = format!("{texto} · {}", t.projeto);
            }
            let largura = (ui.available_width() - largura_link).max(60.0);
            ui.scope(|ui| {
                ui.set_max_width(largura);
                ui.add(egui::Label::new(RichText::new(&texto).color(p.texto).size(13.5)).truncate()).on_hover_text(&texto);
            });
            let r = tema::link_com(ui, "Trazer de volta", conectado);
            if r.on_hover_text(if daily { "Volta para a daily de hoje" } else { "Volta para esta sprint" }).clicked() {
                voltar = Some(t.tarefa_id);
            }
        });
    }
    voltar
}

#[cfg(test)]
mod testes {
    use super::*;

    fn slide(id: i64, grupo: &str, partes: &[&str], secao: i64, projeto: &str, nota: &str) -> api::Slide {
        api::Slide {
            tarefa_id: id,
            titulo: format!("Tarefa {id}"),
            grupo: grupo.into(),
            coluna: match grupo {
                "concluidas" => "concluido",
                "revisao" => "revisao",
                "aguardando" => "aguardando",
                _ => "trabalhando",
            }
            .into(),
            partes: partes.iter().map(|p| p.to_string()).collect(),
            secao: projeto.into(),
            secao_id: secao,
            projeto: projeto.into(),
            nota: nota.into(),
            ..Default::default()
        }
    }

    #[test]
    fn parte_so_quando_nao_e_de_hoje() {
        assert_eq!(parte_do_slide(&slide(1, "concluidas", &["Ontem"], 1, "a", "")), Some("Ontem"));
        assert_eq!(parte_do_slide(&slide(1, "concluidas", &["Ontem", "Hoje"], 1, "a", "")), None);
        assert_eq!(parte_do_slide(&slide(1, "concluidas", &["Sexta"], 1, "a", "")), Some("Sexta"));
        assert_eq!(parte_do_slide(&slide(1, "trabalhando", &[], 1, "a", "")), None);
    }

    #[test]
    fn texto_em_blocos_por_projeto() {
        let deck = api::Deck {
            slides: vec![
                slide(1, "concluidas", &["Hoje"], 1, "loja-web", "Correção da leitura do código de barras."),
                slide(2, "concluidas", &["Ontem"], 1, "loja-web", "Validando o dígito verificador."),
                slide(3, "revisao", &["Hoje"], 2, "hub", "1. Campo novo no instalador.\n2. A central grava o endereço.\n- Falta subir para revisão."),
                slide(4, "trabalhando", &[], 2, "hub", ""),
                slide(5, "aguardando", &["Hoje"], 3, "app-lojista", ""),
            ],
            ..Default::default()
        };
        let esperado = concat!(
            "loja-web\n",
            "- Tarefa 1. Correção da leitura do código de barras.\n",
            "- Tarefa 2 (ontem). Validando o dígito verificador.\n",
            "\n",
            "hub\n",
            "- Tarefa 3 (em revisão)\n",
            "  1. Campo novo no instalador.\n",
            "  2. A central grava o endereço.\n",
            "  - Falta subir para revisão.\n",
            "- Tarefa 4 (em andamento)\n",
            "\n",
            "Esperando minha resposta\n",
            "- Tarefa 5 (app-lojista)"
        );
        assert_eq!(texto_em_blocos(&deck), esperado);
        // Um projeto só: sem a linha do nome e sem o projeto entre parênteses.
        let um = api::Deck {
            slides: vec![slide(1, "concluidas", &["Hoje"], 1, "loja-web", "Pronto."), slide(5, "aguardando", &["Hoje"], 1, "loja-web", "")],
            ..Default::default()
        };
        assert_eq!(texto_em_blocos(&um), "- Tarefa 1. Pronto.\n\nEsperando minha resposta\n- Tarefa 5");
        // Sem nota de hoje, a de ontem vale para quem parou.
        let mut s = slide(6, "concluidas", &["Ontem"], 1, "loja-web", "");
        s.nota_anterior = Some(api::NotaAnterior { texto: "Ficou pronto.".into(), periodo: "ontem".into() });
        assert_eq!(linha_do_texto(&s), "- Tarefa 6 (ontem). Ficou pronto.");
    }
}
