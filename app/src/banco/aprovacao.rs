//! O cartão de pedido do agente no painel da tarefa: a instrução que ele
//! quer rodar, o aviso de que o resultado vai para o provedor de IA e os
//! botões. Fica no fluxo do painel (não é modal: não rouba o teclado de quem
//! digita no terminal), sem botão padrão e sem foco automático; o prazo é uma
//! hora fixa, sem contagem regressiva.

use eframe::egui::{self, CornerRadius, FontId, Id, RichText, Sense, Stroke, vec2};

use super::conexao;
use crate::api;
use crate::dados::Resolvida;
use crate::tema::{self, Icone, cores};

/// O que você está fazendo num pedido (por id).
#[derive(Default)]
pub struct Estado {
    pub recusando: bool,
    pub motivo: String,
    focar: bool,
    pub senha: String,
    pub guardar: bool,
    pub erro: Option<String>,
    /// "Usuário ou senha recusados · tente de novo", embaixo do campo.
    pub erro_senha: Option<String>,
    pub focar_senha: bool,
    /// A resposta foi enviada e o núcleo ainda não voltou.
    pub enviando: bool,
}

impl Estado {
    pub fn novo() -> Estado {
        Estado { guardar: true, ..Default::default() }
    }
}

pub enum Resposta {
    Aprovar { senha: String, guardar: bool },
    Recusar { motivo: String },
}

/// O cartão do pedido. `chaveiro`: há chaveiro para guardar a senha pedida.
pub fn cartao(ui: &mut egui::Ui, a: &api::Aprovacao, estado: &mut Estado, chaveiro: bool) -> Option<Resposta> {
    let p = cores();
    let mut resposta = None;
    egui::Frame::new()
        .fill(tema::fundo_tingido(p, p.alerta, tema::claro()))
        .stroke(Stroke::new(1.0, p.alerta.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(tema::RAIO_CARTAO))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 18.0), Sense::hover());
                ui.painter().circle_filled(r.left_center() + vec2(4.0, 0.0), 4.0, p.alerta);
                let quem = if a.agente.is_empty() { "O agente" } else { a.agente.as_str() };
                ui.label(RichText::new(format!("{quem} quer consultar ")).color(p.texto).size(13.5));
                ui.label(tema::texto_forte(&a.conexao, 13.5).color(p.texto));
                let banco = if a.banco.is_empty() { String::new() } else { format!(" · banco {}", a.banco) };
                let limite = if a.limite > 0 { a.limite } else { 200 };
                ui.label(RichText::new(format!("{banco} · só leitura, até {limite} linhas")).color(p.texto).size(13.0));
            });
            ui.add_space(8.0);
            // "Copiar" numa coluna própria à direita do bloco (o SQL não passa
            // por baixo dele): centrado quando o SQL é curto, no topo quando é longo.
            let largura = ui.painter().layout_no_wrap("Copiar".into(), FontId::proportional(13.0), p.texto).size().x + 28.0;
            let bloco = conexao::bloco_sql_com(ui, &a.tipo, &a.sql, 216.0 + 16.0, &format!("sql-pedido-{}", a.id), largura + 6.0, 32.0 + 2.0 * 6.0);
            let y = if bloco.height() <= 60.0 { bloco.center().y - 16.0 } else { bloco.top() + 6.0 };
            let r = egui::Rect::from_min_size(egui::pos2(bloco.right() - 6.0 - largura, y), vec2(largura, 32.0));
            if ui.put(r, |ui: &mut egui::Ui| tema::botao_secundario(ui, "Copiar")).clicked() {
                ui.ctx().copy_text(a.sql.clone());
            }
            if a.precisa_senha && !estado.recusando {
                ui.add_space(10.0);
                ui.label(RichText::new(format!("Senha de {}", a.conexao)).color(p.texto).size(12.5));
                let campo = conexao::campo_senha(ui, &mut estado.senha, "a senha não está guardada", Id::new(("senha-pedido", &a.id)));
                // Senha recusada: o campo volta vazio e ganha o foco uma vez.
                if std::mem::take(&mut estado.focar_senha) {
                    campo.request_focus();
                }
                if campo.changed() {
                    estado.erro_senha = None;
                }
                if let Some(e) = &estado.erro_senha {
                    ui.add_space(4.0);
                    ui.label(RichText::new(e).color(p.erro).size(12.5));
                }
                if chaveiro {
                    tema::caixa_marcar(ui, "Guardar no chaveiro", &mut estado.guardar);
                }
            }
            if let Some(e) = &estado.erro {
                ui.add_space(6.0);
                ui.label(RichText::new(e).color(p.erro).size(12.5));
            }
            ui.add_space(12.0);
            // O aviso e o prazo à esquerda, os botões à direita, na mesma linha.
            let aviso = |ui: &mut egui::Ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new("O resultado vai para o agente e para o provedor de IA.").color(p.texto).size(12.5));
                    if !a.expira_hora.is_empty() {
                        ui.label(tema::texto_forte(format!("Expira às {}", a.expira_hora), 12.5).color(p.texto));
                    }
                });
            };
            if estado.recusando {
                ui.horizontal(|ui| {
                    let largura = (ui.available_width() - 200.0).max(160.0);
                    let id = Id::new(("motivo-pedido", &a.id));
                    let campo = egui::Frame::new()
                        .fill(tema::fundo_campo(p, false))
                        .stroke(Stroke::new(1.0, p.borda))
                        .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
                        .inner_margin(egui::Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut estado.motivo)
                                    .id(id)
                                    .frame(egui::Frame::NONE)
                                    .desired_width(largura - 22.0)
                                    .char_limit(500)
                                    .font(FontId::proportional(13.5))
                                    .hint_text(RichText::new("Dizer ao agente por quê (opcional)").color(p.suave)),
                            )
                        })
                        .inner;
                    // O foco no campo é pedido uma vez, ao abrir.
                    if std::mem::take(&mut estado.focar) {
                        campo.request_focus();
                    }
                    // Enter no motivo recusa (você já escolheu Recusar antes).
                    if campo.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !estado.enviando {
                        resposta = Some(Resposta::Recusar { motivo: estado.motivo.trim().to_string() });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tema::botao_icone(ui, Icone::Fechar, 28.0).on_hover_text("Voltar").clicked() {
                            estado.recusando = false;
                        }
                        ui.add_space(4.0);
                        if tema::botao_alerta(ui, "Recusar").clicked() && !estado.enviando {
                            resposta = Some(Resposta::Recusar { motivo: estado.motivo.trim().to_string() });
                        }
                    });
                });
                ui.add_space(6.0);
                aviso(ui);
            } else {
                ui.horizontal(|ui| {
                    aviso(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rotulo, pode) = if a.precisa_senha { ("Aprovar com esta senha", !estado.senha.is_empty()) } else { ("Aprovar e enviar", true) };
                        let rotulo = if estado.enviando { "Conectando…" } else { rotulo };
                        if tema::botao_principal(ui, rotulo, pode && !estado.enviando).clicked() {
                            resposta = Some(Resposta::Aprovar { senha: estado.senha.clone(), guardar: estado.guardar });
                        }
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Recusar").clicked() {
                            estado.recusando = true;
                            estado.focar = true;
                        }
                    });
                });
            }
        });
    resposta
}

/// A linha que fica depois da decisão, até o próximo pedido do agente.
pub fn resolvida(ui: &mut egui::Ui, r: &Resolvida) {
    let p = cores();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let centro = egui::pos2(rect.left() + 8.0, rect.center().y);
    let texto = match r.resultado.as_str() {
        "aprovada" if r.erro => {
            ui.painter().circle_filled(centro, 3.5, p.erro);
            format!("Você aprovou · a consulta deu erro · {}", r.hora)
        }
        "aprovada" => {
            ui.painter().circle_filled(centro, 3.5, p.ok);
            let linhas = if r.linhas == 1 { "1 linha enviada".to_string() } else { format!("{} linhas enviadas", r.linhas) };
            format!("Você aprovou · {linhas} · {} ms · {}", r.ms, r.hora)
        }
        "recusada" => {
            ui.painter().circle_stroke(centro, 2.75, Stroke::new(1.5, p.suave));
            format!("Você recusou · {}", r.hora)
        }
        "expirou" => {
            ui.painter().circle_stroke(centro, 2.75, Stroke::new(1.5, p.alerta));
            format!("Expirou sem resposta · {}", r.hora)
        }
        _ => {
            ui.painter().circle_stroke(centro, 2.75, Stroke::new(1.5, p.suave));
            format!("O agente desistiu da consulta · {}", r.hora)
        }
    };
    ui.painter().text(egui::pos2(rect.left() + 20.0, rect.center().y), egui::Align2::LEFT_CENTER, texto, FontId::proportional(12.5), p.suave);
}
