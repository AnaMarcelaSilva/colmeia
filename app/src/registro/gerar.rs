//! "Gerar com IA" na sprint fixa: você diz o que quer ("agrupe por cliente e
//! escreva para o gerente"), o núcleo pede ao Claude Code, sem terminal, uma
//! proposta de títulos, descrições e assuntos, e a página mostra a proposta
//! ao lado de como está. Nada muda até Aceitar; Descartar joga fora.
//!
//! Usa tokens da conta do Claude Code do perfil, e por isso só roda quando
//! você pede. Sem o Claude Code instalado, o botão fica inativo e diz por quê.

use eframe::egui::{self, CornerRadius, Id, RichText, Stroke, vec2};

use super::{Registro, Resposta, em_segundo_plano};
use crate::api;
use crate::teclas;
use crate::tema::{self, Icone, TipoAviso, cores};

const EXEMPLOS: [&str; 2] = ["Agrupe por cliente e escreva para o gerente", "Títulos curtos, uma frase por tarefa"];

/// O painel aberto e em que pé está.
pub(super) struct GerarIa {
    pedido: String,
    focar: bool,
    etapa: Etapa,
    erro: Option<String>,
    composicao: teclas::Composicao,
}

enum Etapa {
    Pedido,
    /// Esperando o Claude Code responder pela sprint `sprint`.
    Gerando {
        sprint: i64,
    },
    Proposta {
        proposta: api::Proposta,
        aceitando: bool,
    },
}

impl Registro {
    /// O botão da página da sprint. Inativo fora da sprint fixa, sem tarefas
    /// ou sem o Claude Code instalado, com o motivo no balão.
    pub(super) fn botao_gerar(&mut self, ui: &mut egui::Ui, deck: Option<&api::Deck>) {
        if self.claude.is_none() && !self.pedindo_ferramentas && self.conectado {
            self.pedindo_ferramentas = true;
            em_segundo_plano(&self.canal.0, ui.ctx(), || {
                Resposta::Ferramentas(api::ferramentas().ok().map(|lista| lista.iter().any(|f| f.id == "claude" && f.instalada)))
            });
        }
        let motivo = if !self.conectado {
            Some("Indisponível sem o núcleo")
        } else if self.claude == Some(false) {
            Some("Precisa do Claude Code instalado: é ele quem escreve a proposta.")
        } else if !deck.is_some_and(|d| d.sprint_id != 0) {
            Some("Só numa sprint fixa (escolha \"sprint\" no período).")
        } else if deck.is_some_and(|d| d.slides.is_empty()) {
            Some("A sprint ainda não tem tarefas.")
        } else {
            None
        };
        let aberto = self.gerar.is_some();
        let resposta = tema::botao_secundario_com(ui, "Gerar com IA", motivo.is_none() && self.claude.is_some() && !aberto);
        let dica = motivo
            .unwrap_or("O Claude Code reescreve títulos, descrições e assuntos do jeito que você pedir. Usa tokens da sua conta; nada muda sem você aceitar.");
        if resposta.on_hover_text(dica).clicked() {
            self.gerar = Some(GerarIa { pedido: String::new(), focar: true, etapa: Etapa::Pedido, erro: None, composicao: Default::default() });
        }
    }

    /// Fecha o painel; com a proposta na tela, ela é descartada no núcleo.
    pub(super) fn fechar_gerar(&mut self, avisar_nucleo: bool) {
        if let Some(GerarIa { etapa: Etapa::Proposta { proposta, .. }, .. }) = self.gerar.take()
            && avisar_nucleo
        {
            let id = proposta.id;
            std::thread::spawn(move || api::descartar_proposta(id));
        }
    }

    /// O painel acima das seções: o pedido, a espera ou os botões da proposta.
    pub(super) fn painel_gerar(&mut self, ui: &mut egui::Ui, escopo: &super::Escopo, deck: &api::Deck) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(g) = self.gerar.as_mut() else { return };
        let id_campo = Id::new("pedido-gerar-sprint");
        let com_foco = ctx.memory(|m| m.has_focus(id_campo));
        let enter = com_foco && ctx.input_mut(|i| g.composicao.tirar_enters(&mut i.events));
        let esc = com_foco && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let (mut gerar, mut fechar, mut aceitar) = (enter, esc, false);
        let moldura = egui::Frame::new()
            .fill(p.superficie_alta)
            .stroke(Stroke::new(1.0, p.destaque.gamma_multiply(0.45)))
            .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
            .inner_margin(egui::Margin::same(16));
        moldura.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::hover());
                tema::desenhar_icone(ui.painter(), r.center(), Icone::Agente, p.destaque);
                ui.label(tema::texto_forte("Gerar sprint com IA", 15.0).color(p.texto));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let aceitando = matches!(g.etapa, Etapa::Proposta { aceitando: true, .. });
                    if !aceitando && tema::botao_icone(ui, Icone::Fechar, 26.0).on_hover_text("Fechar (Esc)").clicked() {
                        fechar = true;
                    }
                });
            });
            match &g.etapa {
                Etapa::Pedido => {
                    ui.label(RichText::new("Diga o que você quer da sprint. O Claude Code propõe títulos, descrições e assuntos para cada tarefa.").color(p.suave).size(13.0));
                    let r = tema::campo_mensagem(ui, &mut g.pedido, 2, 120.0, id_campo, false);
                    if std::mem::take(&mut g.focar) {
                        r.request_focus();
                    }
                    if g.pedido.trim().is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for exemplo in EXEMPLOS {
                                if tema::botao_secundario(ui, exemplo).clicked() {
                                    g.pedido = exemplo.into();
                                    g.focar = true;
                                }
                            }
                        });
                    }
                    if let Some(e) = &g.erro {
                        ui.label(RichText::new(e).color(p.erro).size(13.0));
                    }
                    ui.label(
                        RichText::new("Usa tokens da sua conta do Claude Code. Vai só o que está nesta sprint: títulos, descrições, assuntos e o que aconteceu em cada tarefa.")
                            .color(p.suave)
                            .size(12.0),
                    );
                    ui.horizontal(|ui| {
                        gerar |= tema::botao_principal(ui, "Gerar proposta", !g.pedido.trim().is_empty()).clicked();
                        fechar |= tema::botao_secundario(ui, "Cancelar").clicked();
                    });
                }
                Etapa::Gerando { .. } => {
                    ui.label(RichText::new(format!("“{}”", g.pedido.trim())).color(p.texto).size(13.5));
                    ui.label(RichText::new("O Claude Code está montando a proposta. Pode levar um minuto.").color(p.suave).size(13.0));
                    fechar |= tema::botao_secundario(ui, "Cancelar").on_hover_text("Para de esperar; a resposta, se vier, é jogada fora").clicked();
                }
                Etapa::Proposta { proposta, aceitando } => {
                    ui.label(RichText::new(format!("“{}”", g.pedido.trim())).color(p.texto).size(13.5));
                    let n = proposta.itens.len();
                    let tarefas = if n == 1 { "1 tarefa".to_string() } else { format!("{n} tarefas") };
                    ui.label(
                        RichText::new(format!("A proposta muda {tarefas}, abaixo ao lado de como está. Nada muda até você aceitar, e vale só nesta sprint."))
                            .color(p.suave)
                            .size(13.0),
                    );
                    if let Some(e) = &g.erro {
                        ui.label(RichText::new(e).color(p.erro).size(13.0));
                    }
                    ui.horizontal(|ui| {
                        aceitar = tema::botao_principal(ui, if *aceitando { "Aplicando…" } else { "Aceitar" }, !*aceitando).clicked();
                        fechar |= tema::botao_secundario_com(ui, "Descartar", !*aceitando).clicked();
                    });
                }
            }
        });
        ui.add_space(14.0);
        if fechar {
            self.fechar_gerar(true);
            return;
        }
        if gerar && matches!(g.etapa, Etapa::Pedido) && !g.pedido.trim().is_empty() && self.conectado {
            let (Some(chave), sprint) = (self.chave, deck.sprint_id) else { return };
            g.etapa = Etapa::Gerando { sprint };
            g.erro = None;
            let pedido = g.pedido.trim().to_string();
            let recorte = escopo.recorte;
            em_segundo_plano(&self.canal.0, &ctx, move || Resposta::Proposta { sprint, resultado: api::gerar_sprint(chave.0, recorte, sprint, &pedido) });
        }
        if aceitar && let Etapa::Proposta { proposta, aceitando } = &mut g.etapa {
            *aceitando = true;
            g.erro = None;
            let id = proposta.id;
            em_segundo_plano(&self.canal.0, &ctx, move || Resposta::PropostaAceita(api::aceitar_proposta(id)));
        }
    }

    /// A proposta no lugar das seções: por assunto proposto, cada tarefa
    /// como está à esquerda e como fica à direita. Devolve se mostrou.
    pub(super) fn proposta_lado_a_lado(&self, ui: &mut egui::Ui, deck: &api::Deck) -> bool {
        let Some(GerarIa { etapa: Etapa::Proposta { proposta, .. }, .. }) = &self.gerar else { return false };
        let p = cores();
        // As colunas com o mesmo recuo do conteúdo das molduras (12 de cada lado).
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.columns(2, |c| {
                c[0].label(tema::texto_forte("Agora", 12.5).color(p.suave));
                c[1].label(tema::texto_forte("Proposta", 12.5).color(p.destaque));
            });
            ui.add_space(12.0);
        });
        ui.add_space(4.0);
        for (assunto, itens) in por_assunto(&proposta.itens) {
            ui.add_space(6.0);
            let nome = if assunto.is_empty() { "Sem assunto" } else { assunto };
            ui.label(tema::texto_forte(format!("{nome} · {}", itens.len()), 15.0).color(if assunto.is_empty() { p.suave } else { p.texto }));
            ui.add_space(2.0);
            for it in itens {
                let projeto = deck.slides.iter().find(|s| s.tarefa_id == it.tarefa_id).map_or(it.projeto.as_str(), |s| s.projeto.as_str());
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, p.borda))
                    .corner_radius(CornerRadius::same(tema::RAIO_CARTAO))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.columns(2, |c| {
                            let mudou_assunto = (it.antes.assunto != it.assunto).then_some(it.antes.assunto.as_str());
                            lado(&mut c[0], &it.antes.titulo, &it.antes.descricao, mudou_assunto, false);
                            if !projeto.is_empty() {
                                c[0].with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    tema::etiqueta_ui(ui, projeto, tema::fonte_etiqueta(), p.suave)
                                });
                            }
                            lado(&mut c[1], &it.titulo, &it.descricao, None, true);
                        });
                    });
                ui.add_space(6.0);
            }
        }
        true
    }

    /// As respostas do núcleo para o painel.
    pub(super) fn receber_gerar(&mut self, ctx: &egui::Context, r: Resposta, acoes: &mut Vec<super::Acao>) {
        match r {
            Resposta::Ferramentas(instalado) => {
                self.pedindo_ferramentas = false;
                self.claude = instalado;
            }
            Resposta::Proposta { sprint, resultado } => match (self.gerar.as_mut(), resultado) {
                (Some(g), resultado) if matches!(g.etapa, Etapa::Gerando { sprint: s } if s == sprint) => match resultado {
                    Ok(proposta) => g.etapa = Etapa::Proposta { proposta, aceitando: false },
                    Err(e) => {
                        g.etapa = Etapa::Pedido;
                        g.erro = Some(format!("Não consegui gerar a proposta: {e}"));
                    }
                },
                // Cancelada (ou de outra sprint): a proposta que chegou sai do núcleo.
                (_, Ok(proposta)) => {
                    let id = proposta.id;
                    std::thread::spawn(move || api::descartar_proposta(id));
                }
                _ => {}
            },
            Resposta::PropostaAceita(resultado) => match resultado {
                Ok(()) => {
                    self.gerar = None;
                    acoes.push(super::Acao::Avisar(TipoAviso::Neutro, "Sprint reescrita com a proposta".into()));
                    self.pedir_so_sprint(ctx);
                }
                Err(e) => {
                    if let Some(GerarIa { etapa: Etapa::Proposta { aceitando, .. }, erro, .. }) = self.gerar.as_mut() {
                        *aceitando = false;
                        *erro = Some(format!("Não consegui aplicar: {e}"));
                    }
                }
            },
            _ => {}
        }
    }
}

/// Um lado da comparação: título, descrição e, se mudou, o assunto de antes.
fn lado(ui: &mut egui::Ui, titulo: &str, descricao: &str, assunto_antes: Option<&str>, novo: bool) {
    // Sem a justificação das colunas: o texto fica alinhado à esquerda.
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| lado_alinhado(ui, titulo, descricao, assunto_antes, novo));
}

fn lado_alinhado(ui: &mut egui::Ui, titulo: &str, descricao: &str, assunto_antes: Option<&str>, novo: bool) {
    let p = cores();
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.add(egui::Label::new(tema::texto_forte(titulo, 14.0).color(p.texto)).wrap());
    if descricao.trim().is_empty() {
        ui.label(RichText::new("Sem descrição.").color(p.suave).italics().size(13.5));
    } else {
        ui.add(egui::Label::new(RichText::new(descricao.trim()).color(if novo { p.texto } else { p.suave }).size(13.5)).wrap());
    }
    if let Some(a) = assunto_antes {
        let texto = if a.is_empty() { "Estava sem assunto".to_string() } else { format!("Estava em {a}") };
        ui.label(RichText::new(texto).color(p.suave).size(12.0));
    }
}

/// Os itens agrupados pelo assunto proposto, na ordem em que aparecem; sem
/// assunto no fim.
pub(super) fn por_assunto(itens: &[api::ItemProposta]) -> Vec<(&str, Vec<&api::ItemProposta>)> {
    let mut grupos: Vec<(&str, Vec<&api::ItemProposta>)> = Vec::new();
    for it in itens {
        match grupos.iter_mut().find(|(a, _)| a.to_lowercase() == it.assunto.to_lowercase()) {
            Some((_, lista)) => lista.push(it),
            None => grupos.push((&it.assunto, vec![it])),
        }
    }
    grupos.sort_by_key(|(a, _)| a.is_empty());
    grupos
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(id: i64, assunto: &str) -> api::ItemProposta {
        api::ItemProposta {
            tarefa_id: id,
            titulo: "t".into(),
            descricao: "d".into(),
            assunto: assunto.into(),
            projeto: String::new(),
            antes: Default::default(),
        }
    }

    #[test]
    fn proposta_por_assunto_sem_assunto_no_fim() {
        let itens = [item(1, ""), item(2, "Cliente X"), item(3, "cliente x"), item(4, "Cliente Y")];
        let g: Vec<(&str, Vec<i64>)> = por_assunto(&itens).into_iter().map(|(a, l)| (a, l.iter().map(|i| i.tarefa_id).collect())).collect();
        assert_eq!(g, vec![("Cliente X", vec![2, 3]), ("Cliente Y", vec![4]), ("", vec![1])]);
    }
}
