//! A sprint para a reunião: a escolha da sprint fixa (as setas e as datas) e
//! a lista em resumo, uma linha por tarefa com o título e a descrição da
//! sprint. Os dois só valem na sprint: a tarefa, o quadro e a daily
//! continuam como estão. O clique no título abre o cartão completo embaixo.
//!
//! Os assuntos juntam tarefas de projetos diferentes: a alça de um item (ou
//! a do título de um projeto, com todas as tarefas dele) arrasta até um
//! assunto da barra. O projeto da tarefa não muda.

use eframe::egui::{self, CornerRadius, FontId, Id, RichText, Sense, Stroke, pos2, vec2};

use super::comum::{self, CliqueCartao};
use super::{EdicaoItem, Registro};
use crate::api;
use crate::tema::{self, Icone, cores};

/// AAAA-MM-DD vira DD/MM.
fn curta(data: &str) -> String {
    if data.len() == 10 { format!("{}/{}", &data[8..10], &data[5..7]) } else { data.to_string() }
}

/// AAAA-MM-DD vira DD/MM/AAAA (o formato dos campos).
fn da_tela(data: &str) -> String {
    if data.len() == 10 { format!("{}/{}/{}", &data[8..10], &data[5..7], &data[0..4]) } else { data.to_string() }
}

/// O que está sendo arrastado: as tarefas (uma ou um projeto inteiro) e o
/// rótulo que acompanha o ponteiro (também o nome de um assunto novo).
pub(super) struct Arrasto {
    pub tarefas: Vec<i64>,
    pub rotulo: String,
}

/// A alça de arrastar: seis pontos; o arrasto leva `arrasto()`.
pub(super) fn alca(ui: &mut egui::Ui, dica: &str, arrasto: impl FnOnce() -> Arrasto) {
    let p = cores();
    let (r, resposta) = ui.allocate_exact_size(vec2(14.0, 22.0), Sense::drag());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(dica);
    let cor = if resposta.hovered() || resposta.dragged() { p.texto } else { p.suave };
    for linha in 0..3 {
        for coluna in 0..2 {
            let c = pos2(r.center().x - 2.5 + 5.0 * coluna as f32, r.center().y - 5.0 + 5.0 * linha as f32);
            ui.painter().circle_filled(c, 1.4, cor);
        }
    }
    if resposta.dragged() {
        resposta.dnd_set_drag_payload(arrasto());
    }
}

/// O rótulo que segue o ponteiro durante o arrasto.
pub(super) fn rotulo_do_arrasto(ctx: &egui::Context) {
    let p = cores();
    let (Some(a), Some(ponto)) = (egui::DragAndDrop::payload::<Arrasto>(ctx), ctx.pointer_interact_pos()) else { return };
    let pintor = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("arrasto-assunto")));
    let texto = if a.tarefas.len() == 1 { a.rotulo.clone() } else { format!("{} · {} tarefas", a.rotulo, a.tarefas.len()) };
    let galeria = pintor.layout_no_wrap(texto, FontId::proportional(13.5), p.texto);
    let caixa = egui::Rect::from_min_size(ponto + vec2(14.0, 10.0), galeria.size() + vec2(20.0, 12.0));
    pintor.rect(caixa, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie_alta, Stroke::new(1.0, p.destaque), egui::StrokeKind::Inside);
    pintor.galley(caixa.min + vec2(10.0, 6.0), galeria, p.texto);
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
}

/// Durante o arrasto, a lista rola: com a roda do mouse (que o egui ignora
/// enquanto algo é arrastado) e sozinha com o ponteiro perto da borda de
/// cima ou de baixo. Chamada dentro da área de rolagem.
pub(super) fn rolar_no_arrasto(ui: &egui::Ui) {
    let ctx = ui.ctx();
    let (true, Some(ponto)) = (egui::DragAndDrop::has_payload_of_type::<Arrasto>(ctx), ctx.pointer_hover_pos()) else { return };
    let visivel = ui.clip_rect();
    if !visivel.x_range().contains(ponto.x) {
        return;
    }
    let mut delta = ctx.input(|i| i.smooth_scroll_delta.y);
    const BORDA: f32 = 70.0;
    let perto_do_topo = BORDA - (ponto.y - visivel.top());
    let perto_do_fim = BORDA - (visivel.bottom() - ponto.y);
    if perto_do_topo > 0.0 {
        delta += perto_do_topo.min(BORDA * 1.5) * 0.3;
    } else if perto_do_fim > 0.0 {
        delta -= perto_do_fim.min(BORDA * 1.5) * 0.3;
    }
    if delta != 0.0 {
        ui.scroll_with_delta_animation(vec2(0.0, delta), egui::style::ScrollAnimation::none());
        ctx.request_repaint();
    }
}

/// Um assunto (ou "Sem assunto", "Novo assunto") na barra: aceita o arrasto
/// e devolve o clique e o que foi solto nele.
fn alvo(ui: &mut egui::Ui, texto: &str, suave: bool) -> (egui::Response, Option<std::sync::Arc<Arrasto>>) {
    let p = cores();
    let arrastando = egui::DragAndDrop::has_payload_of_type::<Arrasto>(ui.ctx());
    let galeria = ui.painter().layout_no_wrap(texto.to_string(), FontId::proportional(13.5), if suave { p.suave } else { p.texto });
    let (r, resposta) = ui.allocate_exact_size(galeria.size() + vec2(24.0, 14.0), Sense::click());
    let sobre = arrastando && resposta.contains_pointer();
    let (fundo, borda) = if sobre {
        (p.destaque.gamma_multiply(0.18), p.destaque)
    } else if arrastando {
        (p.superficie, p.destaque.gamma_multiply(0.5))
    } else {
        (p.superficie, p.borda)
    };
    ui.painter().rect(r, CornerRadius::same(tema::RAIO_CONTROLE), fundo, Stroke::new(1.0, borda), egui::StrokeKind::Inside);
    ui.painter().galley(r.min + vec2(12.0, 7.0), galeria, p.texto);
    let solto = resposta.dnd_release_payload::<Arrasto>();
    (resposta, solto)
}

impl Registro {
    /// A barra dos assuntos da sprint fixa: cada assunto recebe o que for
    /// arrastado; o botão direito renomeia ou apaga.
    pub(super) fn barra_de_assuntos(&mut self, ui: &mut egui::Ui, deck: &api::Deck) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let sprint = deck.sprint_id;
        let ativo = self.conectado;
        // (assunto, tarefas, nome do assunto novo): o que mandar ao núcleo.
        let mut mover: Option<(i64, Vec<i64>, Option<String>)> = None;
        let mut renomear = None;
        let mut apagar = None;
        let mut repetir = false;
        let mut nome_pronto = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            ui.label(tema::texto_forte("Assuntos", 15.0).color(p.texto));
            ui.add_space(4.0);
            for a in &deck.assuntos {
                if let Some((id, nome)) = self.nome_assunto.as_mut().filter(|(id, _)| *id == a.id) {
                    nome_pronto = nome_pronto.or(campo_nome(ui, nome, *id));
                    continue;
                }
                let n = deck.slides.iter().filter(|s| s.assunto_id == a.id).count();
                let (r, solto) = alvo(ui, &format!("{} · {n}", a.nome), n == 0);
                if let Some(s) = solto {
                    mover = Some((a.id, s.tarefas.clone(), None));
                }
                r.on_hover_text("Solte aqui uma tarefa ou um projeto. Botão direito: renomear ou apagar.").context_menu(|ui| {
                    if ui.button("Renomear").clicked() {
                        renomear = Some((a.id, a.nome.clone()));
                    }
                    if ui.button("Apagar assunto").clicked() {
                        apagar = Some(a.id);
                    }
                });
            }
            if deck.slides.iter().any(|s| s.assunto_id != 0) {
                let (r, solto) = alvo(ui, "Sem assunto", true);
                r.on_hover_text("Solte aqui para a tarefa voltar ao projeto dela");
                if let Some(s) = solto {
                    mover = Some((0, s.tarefas.clone(), None));
                }
            }
            if let Some((0, nome)) = self.nome_assunto.as_mut() {
                nome_pronto = nome_pronto.or(campo_nome(ui, nome, 0));
            } else {
                let (r, solto) = alvo(ui, "+ Novo assunto", true);
                if let Some(s) = solto {
                    mover = Some((0, s.tarefas.clone(), Some(s.rotulo.clone())));
                } else if r.on_hover_text("Criar um assunto; soltar aqui cria um com o nome do que foi arrastado").clicked() && ativo {
                    self.nome_assunto = Some((0, String::new()));
                }
            }
            if deck.assuntos.is_empty() {
                ui.add_space(4.0);
                repetir = tema::link_com(ui, "Repetir os da sprint anterior", ativo).clicked();
            }
        });
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "Arraste pela alça uma tarefa ou um projeto inteiro até um assunto. O projeto da tarefa não muda, e assunto vazio não aparece na apresentação.",
            )
            .color(p.suave)
            .size(12.5),
        );
        if renomear.is_some() {
            self.nome_assunto = renomear;
        }
        if let Some(salvar) = nome_pronto
            && let Some((id, nome)) = self.nome_assunto.take()
        {
            let nome = nome.trim().to_string();
            if salvar && !nome.is_empty() {
                self.mexer_assuntos(&ctx, move || {
                    if id == 0 { api::criar_assunto(sprint, &nome).map(|_| String::new()) } else { api::renomear_assunto(id, &nome).map(|_| String::new()) }
                });
            }
        }
        if let Some(id) = apagar {
            self.mexer_assuntos(&ctx, move || api::remover_assunto(id).map(|_| "Assunto apagado; as tarefas voltaram aos projetos".to_string()));
        }
        if repetir {
            self.mexer_assuntos(&ctx, move || {
                api::repetir_assuntos(sprint)
                    .map(|r| if r.assuntos == 0 { "A sprint anterior não tem assuntos novos para trazer".into() } else { String::new() })
            });
        }
        if let Some((assunto, tarefas, novo)) = mover
            && ativo
        {
            self.mexer_assuntos(&ctx, move || {
                let assunto = match novo {
                    Some(nome) => api::criar_assunto(sprint, &nome.chars().take(80).collect::<String>())?.id,
                    None => assunto,
                };
                api::assunto_das_tarefas(sprint, &tarefas, assunto).map(|_| String::new())
            });
        }
    }

    /// As setas entre as sprints, as datas da escolhida e "Editar datas".
    pub(super) fn escolha_da_sprint(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(lista) = self.sprints.as_ref() else { return };
        let Some(pos) = self.sprint_escolhida.and_then(|id| lista.sprints.iter().position(|s| s.id == id)) else { return };
        let s = lista.sprints[pos].clone();
        let anterior = pos.checked_sub(1).map(|i| lista.sprints[i].id);
        let proxima = lista.sprints.get(pos + 1).map(|x| x.id);
        let atual = lista.atual;

        if let Some((de, ate)) = self.datas_sprint.as_mut() {
            let (mut salvar, mut cancelar) = (false, false);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    tema::campo(ui, "Começa em", de, "DD/MM/AAAA");
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    salvar = tema::campo(ui, "Termina em", ate, "DD/MM/AAAA").lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(19.0);
                    ui.horizontal(|ui| {
                        salvar |= tema::botao_principal(ui, "Salvar", self.conectado).clicked();
                        cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
                    });
                });
            });
            ui.add_space(4.0);
            ui.label(RichText::new("A próxima sprint segue estas datas: começa no dia seguinte, com a mesma duração.").color(p.suave).size(12.5));
            if let Some(e) = &self.erro_datas {
                ui.label(RichText::new(e).color(p.erro).size(13.0));
            }
            if cancelar || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.datas_sprint = None;
                self.erro_datas = None;
            } else if salvar {
                self.salvar_datas(&ctx);
            }
            return;
        }

        let mut ir = None;
        ui.horizontal(|ui| {
            if tema::botao_icone_com(ui, Icone::Anterior, 30.0, anterior.is_some()).on_hover_text("Sprint anterior").clicked() {
                ir = anterior;
            }
            ui.label(tema::texto_forte(format!("{} a {}", curta(&s.inicio), curta(&s.fim)), 15.0).color(p.texto));
            if tema::botao_icone_com(ui, Icone::Proximo, 30.0, proxima.is_some()).on_hover_text("Próxima sprint").clicked() {
                ir = proxima;
            }
            let quando = if s.id == atual {
                "sprint atual"
            } else if proxima.is_none() || s.inicio > lista.sprints.iter().find(|x| x.id == atual).map(|x| x.inicio.clone()).unwrap_or_default() {
                "sprint futura"
            } else {
                "sprint passada"
            };
            ui.label(RichText::new(quando).color(p.suave).size(13.0));
            ui.add_space(12.0);
            if tema::link_com(ui, "Editar datas", self.conectado).on_hover_text("Mudar o começo e o fim desta sprint").clicked() {
                self.datas_sprint = Some((da_tela(&s.inicio), da_tela(&s.fim)));
                self.erro_datas = None;
            }
            if s.id != atual {
                ui.add_space(8.0);
                if tema::link_com(ui, "Ir para a atual", true).clicked() {
                    ir = Some(atual);
                }
            }
        });
        if let Some(id) = ir {
            self.escolher_sprint(&ctx, id);
        }
    }

    /// A lista em resumo de uma seção: título, estado e descrição de cada
    /// tarefa. Na sprint fixa, o lápis edita o título e a descrição dela.
    pub(super) fn lista(&mut self, ui: &mut egui::Ui, deck: &api::Deck, slides: &[&api::Slide], com_projeto: bool) -> Option<CliqueCartao> {
        let p = cores();
        let editavel = deck.sprint_id != 0 && self.conectado;
        let mut clique = None;
        for s in slides {
            let id = s.tarefa_id;
            let aberto = self.abertos.contains(&id);
            let editando = self.edicao.as_ref().is_some_and(|e| e.tarefa == id);
            let (mut alternar, mut editar) = (false, false);
            let mut pedir = None;
            let moldura = egui::Frame::new()
                .fill(p.superficie_alta)
                .stroke(Stroke::new(1.0, if editando { p.destaque.gamma_multiply(0.55) } else { p.borda }))
                .corner_radius(CornerRadius::same(tema::RAIO_CARTAO))
                .inner_margin(egui::Margin::symmetric(16, 12));
            let resposta = moldura.show(ui, |ui| {
                ui.set_width(ui.available_width());
                if editando {
                    self.formulario(ui, id);
                    return;
                }
                // O título, com o lápis e "Ver detalhes" à direita.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let rotulo = if aberto { "Esconder detalhes" } else { "Ver detalhes" };
                        alternar = tema::link_com(ui, rotulo, true).clicked();
                        if editavel && !s.removida {
                            ui.add_space(8.0);
                            let r = tema::link_com(ui, "Pedir ao agente", true).on_hover_text("O agente pode escrever a descrição para a reunião");
                            if r.clicked() {
                                pedir = Some(r.rect);
                            }
                        }
                        if editavel && !s.removida {
                            ui.add_space(4.0);
                            editar = tema::botao_icone(ui, Icone::Lapis, 28.0).on_hover_text("Editar o título e a descrição só nesta sprint").clicked();
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            if editavel && !s.removida {
                                let rotulo = s.titulo.clone();
                                alca(ui, "Arraste até um assunto", || Arrasto { tarefas: vec![id], rotulo });
                            }
                            let titulo = egui::Label::new(tema::texto_forte(&s.titulo, 15.0).color(p.texto)).truncate().sense(Sense::click());
                            alternar |= ui.add(titulo).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
                        });
                    });
                });
                // Estado, projeto e, se o título mudou, o da tarefa.
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let pilula = comum::pilula_do_slide(s, false);
                    let (r, _) = ui.allocate_exact_size(vec2(pilula.largura(ui.painter()), 22.0), Sense::hover());
                    pilula.pintar(ui.painter(), r.min);
                    if com_projeto && !s.projeto.is_empty() {
                        tema::etiqueta_ui(ui, &s.projeto, tema::fonte_etiqueta(), p.suave);
                    }
                    if !s.titulo_original.is_empty() {
                        ui.add(egui::Label::new(RichText::new(format!("Na tarefa: {}", s.titulo_original)).color(p.suave).size(12.5)).truncate());
                    }
                });
                ui.add_space(6.0);
                let nota = comum::sem_linhas_vazias(&s.nota);
                if nota.trim().is_empty() {
                    let texto = if editavel { "Sem descrição. Use o lápis para escrever o que contar na reunião." } else { "Sem descrição." };
                    ui.label(RichText::new(texto).color(p.suave).italics().size(13.5));
                } else {
                    let largura = ui.available_width();
                    let formato = egui::TextFormat { font_id: FontId::proportional(13.5), color: p.texto, ..Default::default() };
                    let galeria = tema::cortar(ui.painter(), &nota, formato, largura, 3, false);
                    let (r, _) = ui.allocate_exact_size(vec2(largura, galeria.size().y), Sense::hover());
                    ui.painter().galley(r.min, galeria, p.texto);
                }
                if aberto && let Some(c) = detalhes(ui, s, self.pedidos.get(&id).map(|r| &r.estado), &mut self.miniaturas) {
                    clique = Some(c);
                }
            });
            if editar {
                self.abrir_edicao(s);
            }
            if let Some(r) = pedir {
                clique = Some(CliqueCartao::Pedir(id, r));
            }
            if alternar && !self.abertos.remove(&id) {
                self.abertos.insert(id);
            }
            if self.rolar_ate == Some(id) {
                ui.scroll_to_rect(resposta.response.rect, Some(egui::Align::Center));
                self.rolar_ate = None;
            }
            ui.add_space(8.0);
        }
        clique
    }

    fn abrir_edicao(&mut self, s: &api::Slide) {
        let titulo_tarefa = if s.titulo_original.is_empty() { s.titulo.clone() } else { s.titulo_original.clone() };
        self.edicao = Some(EdicaoItem {
            tarefa: s.tarefa_id,
            titulo: s.titulo.clone(),
            descricao: s.nota.clone(),
            titulo_tarefa,
            titulo_antes: s.titulo.clone(),
            descricao_antes: s.nota.clone(),
            versao: s.nota_versao.clone(),
            salvando: false,
            focar: true,
            aviso: None,
        });
    }

    /// O título e a descrição da tarefa na sprint, com Salvar e Cancelar.
    fn formulario(&mut self, ui: &mut egui::Ui, tarefa: i64) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(e) = self.edicao.as_mut() else { return };
        let (mut salvar, mut cancelar) = (false, false);
        let r = tema::campo(ui, "Título na sprint", &mut e.titulo, &e.titulo_tarefa.clone());
        if std::mem::take(&mut e.focar) {
            r.request_focus();
        }
        salvar |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(8.0);
        ui.label(RichText::new("Descrição").color(p.suave).size(12.5));
        ui.add_space(2.0);
        tema::campo_multilinha(ui, &mut e.descricao, 3, 220.0, egui::Id::new(("descricao-sprint", tarefa)), e.salvando);
        ui.add_space(6.0);
        ui.label(
            RichText::new("Vale só para esta sprint: a tarefa, o quadro e a daily continuam como estão. A descrição também é a nota do slide.")
                .color(p.suave)
                .size(12.5),
        );
        if let Some(aviso) = &e.aviso {
            ui.add_space(4.0);
            ui.label(RichText::new(aviso).color(p.erro).size(13.0));
        }
        ui.add_space(10.0);
        let mut usar_original = false;
        ui.horizontal(|ui| {
            salvar |= tema::botao_principal(ui, if e.salvando { "Salvando…" } else { "Salvar" }, !e.salvando).clicked();
            cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
            if e.titulo.trim() != e.titulo_tarefa.trim() {
                ui.add_space(8.0);
                usar_original = tema::link_com(ui, "Usar o título da tarefa", !e.salvando).clicked();
            }
        });
        if usar_original {
            e.titulo = e.titulo_tarefa.clone();
        }
        if cancelar || (!e.salvando && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.edicao = None;
        } else if salvar {
            self.salvar_item(&ctx);
        }
    }
}

/// O nome de um assunto sendo escrito: `Some(true)` com Enter (salva),
/// `Some(false)` ao sair do campo de outro jeito (Esc ou clique fora).
fn campo_nome(ui: &mut egui::Ui, nome: &mut String, id: i64) -> Option<bool> {
    let campo = egui::TextEdit::singleline(nome).id(Id::new(("nome-assunto", id))).hint_text("Nome do assunto").desired_width(200.0).char_limit(80);
    let r = ui.add(campo);
    if r.lost_focus() {
        return Some(ui.input(|i| i.key_pressed(egui::Key::Enter)));
    }
    if !r.has_focus() && ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_da_sprint_na_tela() {
        assert_eq!(curta("2026-10-02"), "02/10");
        assert_eq!(da_tela("2026-10-08"), "08/10/2026");
        assert_eq!(curta("x"), "x");
    }
}

/// Os detalhes do item aberto, dentro do mesmo cartão: o que aconteceu
/// na tarefa, o pedido ao agente em andamento e os anexos.
fn detalhes(ui: &mut egui::Ui, s: &api::Slide, estado: Option<&crate::pedido::Estado>, cache: &mut comum::CacheImagens) -> Option<CliqueCartao> {
    let p = cores();
    let mut clique = None;
    ui.add_space(12.0);
    let linha = ui.available_rect_before_wrap();
    ui.painter().hline(linha.x_range(), linha.top(), Stroke::new(1.0, p.borda));
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Na tarefa").color(p.suave).size(12.5));
        if let Some(e) = estado {
            let pilula = crate::pedido::pilula(e);
            let (r, resposta) = ui.allocate_exact_size(vec2(pilula.largura(ui.painter()), 22.0), Sense::click());
            pilula.pintar(ui.painter(), r.min);
            if resposta.on_hover_text(&e.longo).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clique = Some(CliqueCartao::Pedir(s.tarefa_id, r));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tema::link_com(ui, "Ver na apresentação", true).clicked() {
                clique = Some(CliqueCartao::Abrir(s.tarefa_id));
            }
        });
    });
    let topicos = comum::topicos(s);
    if topicos.is_empty() {
        ui.label(RichText::new("Nada registrado na tarefa neste período.").color(p.suave).size(13.0));
    }
    for t in &topicos {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(8.0, 18.0), Sense::hover());
            ui.painter().circle_filled(pos2(r.left() + 3.0, r.center().y), 2.0, p.suave);
            ui.add(egui::Label::new(RichText::new(*t).color(p.texto).size(13.5)).wrap());
        });
    }
    if !s.anexos.is_empty() {
        ui.add_space(10.0);
        let largura = (comum::LADO_MINIATURA + 8.0) * s.anexos.len().min(4) as f32;
        let (r, resposta) = ui.allocate_exact_size(vec2(largura, comum::LADO_MINIATURA), Sense::click());
        let pintor = ui.painter().clone();
        comum::miniaturas(ui, &pintor, cache, s, r.min);
        if resposta.on_hover_text("Ver na apresentação").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            clique = Some(CliqueCartao::Abrir(s.tarefa_id));
        }
    }
    clique
}
