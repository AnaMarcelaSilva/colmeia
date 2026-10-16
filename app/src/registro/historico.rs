//! Histórico: as tarefas concluídas por dia (Hoje, Ontem, as datas) à
//! esquerda e, à direita, a escolhida com o que aconteceu nela. Os dados são
//! as páginas da linha do tempo do núcleo; rolar até o fim da lista traz os
//! dias anteriores. A lista é virtualizada e montada só quando chega página.

use eframe::egui::{self, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};

use std::collections::HashSet;

use super::comum::{self, texto_cortado};
use super::{Acao, Registro, linha};
use crate::api;
use crate::dados::Tarefa;
use crate::tema::{self, Icone, Pilula, cores, forte};

const LISTA: f32 = 340.0;
const BUSCA: f32 = 34.0;
const DIA: f32 = 34.0;
const LINHA: f32 = 50.0;
const ESPECIAL: f32 = 44.0;
const DIA_DO_EVENTO: f32 = 26.0;

/// Uma tarefa concluída, como a lista mostra.
pub(super) struct Concluida {
    tarefa: i64,
    titulo: String,
    /// O título em minúsculas, para a busca.
    busca: String,
    projeto: String,
    hora: String,
}

pub(super) struct DiaConcluidas {
    titulo: String,
    tarefas: Vec<Concluida>,
}

/// O que identifica as páginas carregadas: muda quando chega uma.
pub(super) type ChaveDias = (usize, usize, i64);

pub(super) fn chave(dias: &[api::Dia]) -> ChaveDias {
    (dias.len(), dias.iter().map(|d| d.itens.len()).sum(), dias.first().and_then(|d| d.itens.first()).map_or(0, |i| i.evento))
}

/// As concluídas por dia: a conclusão mais recente de cada tarefa que
/// continua no Concluído (reaberta ou removida, ela sai do histórico).
pub(super) fn concluidas(dias: &[api::Dia]) -> Vec<DiaConcluidas> {
    let mut vistas = HashSet::new();
    dias.iter()
        .filter_map(|dia| {
            let tarefas: Vec<Concluida> = dia
                .itens
                .iter()
                .filter(|i| i.tipo == "concluiu" && i.tarefa_id != 0 && !i.removida && i.coluna == "concluido" && vistas.insert(i.tarefa_id))
                .map(|i| Concluida {
                    tarefa: i.tarefa_id,
                    titulo: i.titulo.clone(),
                    busca: i.titulo.to_lowercase(),
                    projeto: i.projeto.clone(),
                    hora: i.hora.clone(),
                })
                .collect();
            (!tarefas.is_empty()).then(|| DiaConcluidas { titulo: dia.titulo.clone(), tarefas })
        })
        .collect()
}

enum Fileira {
    Dia(usize),
    Tarefa(usize, usize),
    Carregando,
    Inicio,
    SemResultado,
}

impl Registro {
    pub(super) fn pagina_historico(&mut self, ui: &mut egui::Ui, tarefas: &[Tarefa], um_projeto: bool, acoes: &mut Vec<Acao>) {
        let ctx = ui.ctx().clone();
        if let Some(e) = self.erro.clone()
            && !api::erro_de_conexao(&e)
        {
            if comum::faixa_erro(ui, &format!("Não consegui carregar o histórico: {e}")) {
                self.pedir_pagina(&ctx, true);
            }
            ui.add_space(12.0);
            if self.dias.is_empty() {
                return;
            }
        }
        let chave = chave(&self.dias);
        if self.concluidas.as_ref().is_none_or(|(c, _)| *c != chave) {
            self.concluidas = Some((chave, concluidas(&self.dias)));
        }
        let vazio = self.concluidas.as_ref().is_some_and(|(_, d)| d.is_empty());
        if self.carregou && self.proximo == 0 && vazio {
            let texto = "Quando uma tarefa chega ao Concluído, ela aparece aqui, no dia em que terminou, com tudo o que aconteceu nela.";
            if comum::vazio(ui, "Nada concluído ainda", texto, Some("Ir ao Quadro")) {
                acoes.push(Acao::IrParaQuadro);
            }
            return;
        }

        let area = ui.available_rect_before_wrap();
        let esquerda = Rect::from_min_size(area.min, vec2(LISTA, area.height()));
        let direita = Rect::from_min_max(pos2(esquerda.right() + 16.0, area.top()), area.max);
        let mut lista = ui.new_child(UiBuilder::new().max_rect(esquerda).layout(egui::Layout::top_down(egui::Align::Min)));
        self.lista_historico(&mut lista, um_projeto);
        let escolhida = self.escolhida();
        let mut detalhe = ui.new_child(UiBuilder::new().max_rect(direita).layout(egui::Layout::top_down(egui::Align::Min)));
        match escolhida {
            Some((dia, hora)) => self.detalhe_historico(&mut detalhe, &dia, &hora, tarefas, acoes),
            None => {
                detalhe.painter().text(
                    direita.center_top() + vec2(0.0, 60.0),
                    egui::Align2::CENTER_CENTER,
                    "Escolha uma tarefa para ver o que aconteceu nela.",
                    FontId::proportional(13.5),
                    cores().suave,
                );
            }
        }
        ui.allocate_rect(area, Sense::hover());
    }

    /// A tarefa escolhida (a primeira da lista, de começo) e o dia e a hora
    /// em que ela foi concluída.
    fn escolhida(&mut self) -> Option<(String, String)> {
        let dias = &self.concluidas.as_ref()?.1;
        let achar = |id: i64| dias.iter().find_map(|d| d.tarefas.iter().find(|t| t.tarefa == id).map(|t| (d.titulo.clone(), t.hora.clone())));
        if let Some(achada) = self.selecionada.and_then(achar) {
            return Some(achada);
        }
        let primeira = dias.first()?.tarefas.first()?.tarefa;
        self.selecionada = Some(primeira);
        achar(primeira)
    }

    fn lista_historico(&mut self, ui: &mut egui::Ui, um_projeto: bool) {
        let p = cores();
        let ctx = ui.ctx().clone();
        // A busca: uma pílula com a lupa desenhada e o número à direita.
        let total: usize = self.concluidas.as_ref().map_or(0, |(_, d)| d.iter().map(|d| d.tarefas.len()).sum());
        let (caixa, _) = ui.allocate_exact_size(vec2(ui.available_width(), BUSCA), Sense::hover());
        ui.painter().rect(caixa, CornerRadius::same(17), p.superficie, Stroke::new(1.0, p.borda), StrokeKind::Inside);
        lupa(ui.painter(), pos2(caixa.left() + 16.0, caixa.center().y), p.suave);
        let contagem = if total == 1 { "1 concluída".to_string() } else { format!("{total} concluídas") };
        let numero = ui.painter().layout_no_wrap(contagem, FontId::proportional(11.5), p.suave);
        let fim_texto = caixa.right() - 14.0 - numero.size().x - 8.0;
        ui.painter().galley(pos2(caixa.right() - 14.0 - numero.size().x, caixa.center().y - numero.size().y / 2.0), numero, p.suave);
        let campo = Rect::from_min_max(pos2(caixa.left() + 30.0, caixa.top() + 4.0), pos2(fim_texto, caixa.bottom() - 4.0));
        ui.put(
            campo,
            egui::TextEdit::singleline(&mut self.busca)
                .frame(egui::Frame::NONE)
                .vertical_align(egui::Align::Center)
                .font(FontId::proportional(13.5))
                .hint_text(egui::RichText::new("Buscar nas concluídas").color(p.suave)),
        );
        ui.add_space(6.0);
        if self.novidades > 0 {
            let texto = if self.novidades == 1 { "1 novidade · mostrar".to_string() } else { format!("{} novidades · mostrar", self.novidades) };
            if tema::link_com(ui, &texto, true).clicked() {
                self.ir_ao_topo = true;
                self.novidades = 0;
                self.pedir_pagina(&ctx, true);
            }
        }

        let busca = self.busca.trim().to_lowercase();
        let Some((_, dias)) = &self.concluidas else { return };
        let passa = |t: &Concluida| busca.is_empty() || t.busca.contains(&busca);
        // Fileiras com alturas fixas, sem montar texto.
        let mut fileiras: Vec<(f32, f32, Fileira)> = Vec::new();
        let mut y = 0.0;
        for (d, dia) in dias.iter().enumerate() {
            let mut cabecalho = false;
            for (i, t) in dia.tarefas.iter().enumerate() {
                if !passa(t) {
                    continue;
                }
                if !cabecalho {
                    fileiras.push((y, DIA, Fileira::Dia(d)));
                    y += DIA;
                    cabecalho = true;
                }
                fileiras.push((y, LINHA, Fileira::Tarefa(d, i)));
                y += LINHA + 2.0;
            }
        }
        if fileiras.is_empty() && self.carregou && self.proximo == 0 {
            fileiras.push((y, ESPECIAL, Fileira::SemResultado));
            y += ESPECIAL;
        }
        if self.proximo > 0 || !self.carregou {
            fileiras.push((y, ESPECIAL, Fileira::Carregando));
            y += ESPECIAL;
        } else if !self.criado_em.is_empty() {
            fileiras.push((y, ESPECIAL, Fileira::Inicio));
            y += ESPECIAL;
        }
        let altura = y;

        let mut rolagem = egui::ScrollArea::vertical().id_salt("historico").auto_shrink(false);
        if std::mem::take(&mut self.ir_ao_topo) {
            rolagem = rolagem.vertical_scroll_offset(0.0);
        }
        let mut pedir_mais = false;
        let mut escolher = None;
        let selecionada = self.selecionada;
        let saida = rolagem.show_viewport(ui, |ui, janela| {
            ui.set_height(altura);
            let origem = ui.max_rect().min;
            let largura = ui.available_width() - 6.0;
            for (fy, fh, fileira) in &fileiras {
                if fy + fh < janela.min.y || *fy > janela.max.y {
                    continue;
                }
                let rect = Rect::from_min_size(origem + vec2(0.0, *fy), vec2(largura, *fh));
                match fileira {
                    Fileira::Dia(d) => cabecalho_do_dia(ui.painter(), rect, &dias[*d], &busca),
                    Fileira::Tarefa(d, i) => {
                        let t = &dias[*d].tarefas[*i];
                        if linha_da_tarefa(ui, rect, t, selecionada == Some(t.tarefa), um_projeto).clicked() {
                            escolher = Some(t.tarefa);
                        }
                    }
                    Fileira::Carregando => {
                        let texto = if self.carregou { "Carregando dias anteriores…" } else { "Carregando…" };
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, texto, FontId::proportional(12.5), p.suave);
                        pedir_mais = true;
                    }
                    Fileira::Inicio => {
                        let texto = format!("Início do histórico deste perfil (criado em {}).", self.criado_em);
                        comum::texto_cortado(
                            ui.painter(),
                            rect.left_center() + vec2(10.0, 0.0),
                            &texto,
                            FontId::proportional(12.0),
                            p.suave,
                            rect.width() - 20.0,
                        );
                    }
                    Fileira::SemResultado => {
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Nada com essa busca.", FontId::proportional(13.0), p.suave);
                    }
                }
            }
        });
        if let Some(id) = escolher {
            self.selecionada = Some(id);
        }
        self.no_topo = saida.state.offset.y < 4.0;
        if pedir_mais && !self.carregando && self.carregou {
            self.pedir_pagina(&ctx, false);
        }
    }

    /// A tarefa escolhida: título, branch, Reabrir e Abrir tarefa, os
    /// números e "O que aconteceu", dia a dia.
    fn detalhe_historico(&mut self, ui: &mut egui::Ui, dia: &str, hora: &str, tarefas: &[Tarefa], acoes: &mut Vec<Acao>) {
        let p = cores();
        let Some(id) = self.selecionada else { return };
        let tarefa = tarefas.iter().find(|t| t.id == id);
        let area = ui.max_rect();
        ui.painter().rect(area, CornerRadius::same(16), p.superficie.gamma_multiply(0.7), Stroke::new(1.0, p.borda.gamma_multiply(0.6)), StrokeKind::Inside);
        let mut ui = ui.new_child(UiBuilder::new().max_rect(area.shrink2(vec2(18.0, 16.0))).layout(egui::Layout::top_down(egui::Align::Min)));
        let ui = &mut ui;
        let titulo = self
            .concluidas
            .as_ref()
            .and_then(|(_, d)| d.iter().flat_map(|d| &d.tarefas).find(|t| t.tarefa == id))
            .map_or_else(String::new, |t| t.titulo.clone());
        // Os eventos da tarefa nos dias carregados, do mais novo ao mais antigo.
        let dias: Vec<(&str, linha::DiaDaTarefa)> = self.dias.iter().filter_map(|d| linha::da_tarefa(d, id).map(|t| (d.titulo.as_str(), t))).collect();

        // Cabeçalho: título e número; a etiqueta da coluna e a branch; os botões.
        ui.horizontal(|ui| {
            ui.set_height(48.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if tema::botao_principal(ui, "Abrir tarefa", true).clicked() {
                    acoes.push(Acao::AbrirTarefa { tarefa: id, agente: None, lousa: false });
                }
                ui.add_space(8.0);
                let reabrir = tema::botao_secundario_com(ui, "Reabrir", self.conectado).on_hover_text("Volta para o Backlog");
                if reabrir.clicked() {
                    acoes.push(Acao::Reabrir(id));
                }
                ui.add_space(12.0);
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
                let pintor = ui.painter();
                let numero = format!("#{id}");
                let largura_numero = pintor.layout_no_wrap(numero.clone(), FontId::proportional(12.5), p.suave).size().x;
                let cortou = texto_cortado(pintor, pos2(rect.left(), rect.top() + 12.0), &titulo, forte(17.0), p.texto, rect.width() - largura_numero - 10.0);
                let largura_titulo =
                    tema::cortar(pintor, &titulo, egui::TextFormat::simple(forte(17.0), p.texto), rect.width() - largura_numero - 10.0, 1, true).size().x;
                pintor.text(
                    pos2(rect.left() + largura_titulo + 8.0, rect.top() + 13.0),
                    egui::Align2::LEFT_CENTER,
                    &numero,
                    FontId::proportional(12.5),
                    p.suave,
                );
                if cortou {
                    ui.interact(Rect::from_min_size(rect.min, vec2(largura_titulo, 26.0)), Id::new(("titulo-historico", id)), Sense::hover())
                        .on_hover_text(&titulo);
                }
                let mut x = rect.left();
                let y = rect.top() + 28.0;
                let pilula = Pilula { texto: "Concluída", cor: p.destaque, cheio: true, ponto: true, grande: false };
                x = pilula.pintar(pintor, pos2(x, y - 1.0)).right() + 8.0;
                if let Some(t) = tarefa.filter(|t| !t.branch.is_empty()) {
                    let tipo = tema::tipo_da_branch(&t.branch);
                    if let Some((nome, cor)) = tipo {
                        x = tema::etiqueta(pintor, pos2(x, y), nome, forte(11.0), cor).right() + 8.0;
                    }
                    if tipo.is_none_or(|(nome, _)| nome != t.branch) {
                        texto_cortado(pintor, pos2(x, y + 9.5), &t.branch, FontId::monospace(11.0), p.suave, rect.right() - x);
                    }
                }
            });
        });
        ui.add_space(12.0);

        // Os números: quando concluiu (e quando foi criada, se o dia está
        // carregado) e os agentes.
        let criada =
            self.dias.iter().find_map(|d| d.itens.iter().find(|i| i.tarefa_id == id && i.tipo == "criou").map(|i| (d.titulo.as_str(), i.hora.as_str())));
        let quando = format!("{}, {hora}", dia_curto(dia));
        let sub_quando = criada.map(|(d, h)| format!("criada {}, {h}", dia_curto(d)));
        let agentes: Vec<&crate::dados::AgenteTela> = tarefa.map(|t| t.agentes.iter().collect()).unwrap_or_default();
        let (valor_agentes, sub_agentes) = match agentes.as_slice() {
            [] => ("Nenhum".to_string(), None),
            [um] => (um.nome(), Some(um.papel.clone())),
            [um, outros @ ..] => {
                let nomes: Vec<String> = outros.iter().map(|a| a.nome()).collect();
                let papeis: Vec<&str> = agentes.iter().map(|a| a.papel.as_str()).collect();
                (um.nome(), Some(format!("+ {} · {}", nomes.join(", "), papeis.join(" · "))))
            }
        };
        let eventos: usize = dias.iter().map(|(_, d)| d.eventos.len()).sum();
        let sub_eventos = if dias.len() == 1 { "em 1 dia".to_string() } else { format!("em {} dias", dias.len()) };
        ui.horizontal(|ui| {
            let largura = (ui.available_width() - 16.0) / 3.0;
            dado(ui, largura, "Concluída", &quando, sub_quando.as_deref());
            dado(ui, largura, "Agentes", &valor_agentes, sub_agentes.as_deref());
            dado(ui, largura, "Eventos", &eventos.to_string(), Some(&sub_eventos));
        });
        ui.add_space(14.0);
        let (linha_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(linha_rect, 0, p.borda.gamma_multiply(0.7));
        ui.add_space(10.0);

        // "O que aconteceu", com O que importa | Tudo.
        let visiveis: usize = dias.iter().map(|(_, d)| d.eventos.iter().filter(|e| !self.so_importa || e.importa()).count()).sum();
        ui.horizontal(|ui| {
            ui.label(tema::texto_forte("O que aconteceu", 14.5).color(p.texto));
            ui.label(egui::RichText::new(if visiveis == 1 { "1 evento".to_string() } else { format!("{visiveis} eventos") }).size(12.0).color(p.suave));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(i) = tema::segmentado(ui, &["O que importa", "Tudo"], if self.so_importa { 0 } else { 1 }) {
                    self.so_importa = i == 0;
                }
            });
        });
        ui.add_space(6.0);
        let so_importa = self.so_importa;
        let mut visor = None;
        egui::ScrollArea::vertical().id_salt(("eventos-historico", id)).auto_shrink(false).show(ui, |ui| {
            let largura = ui.available_width() - comum::MARGEM_ROLAGEM;
            for (titulo_dia, d) in &dias {
                let altura = d.altura(so_importa);
                if altura == 0.0 {
                    continue;
                }
                let (rect, _) = ui.allocate_exact_size(vec2(largura, DIA_DO_EVENTO + altura), Sense::hover());
                ui.painter().text(pos2(rect.left() + 8.0, rect.top() + 14.0), egui::Align2::LEFT_CENTER, titulo_dia, forte(11.5), p.suave);
                if let Some(n) = linha::desenhar(ui, pos2(rect.left() + 8.0, rect.top() + DIA_DO_EVENTO), largura - 8.0, d, so_importa, &mut self.miniaturas) {
                    let a = &d.anexos[n];
                    visor = Some((a.id, a.texto.to_string(), a.hora.to_string()));
                }
                ui.add_space(4.0);
            }
            if self.proximo > 0 {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Os dias mais antigos aparecem ao rolar a lista de concluídas.").size(12.0).color(p.suave));
            }
        });
        if let Some((anexo, texto, hora)) = visor {
            let ctx = ui.ctx().clone();
            self.abrir_visor(&ctx, anexo, &texto, &hora, id);
        }
    }
}

/// O dia no meio da frase: só a primeira parte do título ("Hoje · sexta,
/// 16 de outubro" vira "hoje"), com a inicial minúscula.
fn dia_curto(titulo: &str) -> String {
    let parte = titulo.split(" · ").next().unwrap_or(titulo);
    let mut c = parte.chars();
    match c.next() {
        Some(primeira) => primeira.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Um número da tarefa num cartãozinho: rótulo, valor e, se houver, a linha de baixo.
fn dado(ui: &mut egui::Ui, largura: f32, rotulo: &str, valor: &str, sub: Option<&str>) {
    let p = cores();
    let (rect, _) = ui.allocate_exact_size(vec2(largura, 64.0), Sense::hover());
    let pintor = ui.painter();
    pintor.rect(rect, CornerRadius::same(12), p.superficie_alta, Stroke::new(1.0, p.borda.gamma_multiply(0.7)), StrokeKind::Inside);
    pintor.line_segment([rect.left_top() + vec2(12.0, 1.0), rect.right_top() + vec2(-12.0, 1.0)], Stroke::new(1.0, p.luz));
    let x = rect.left() + 12.0;
    let largura = rect.width() - 24.0;
    texto_cortado(pintor, pos2(x, rect.top() + 16.0), rotulo, FontId::proportional(11.5), p.suave, largura);
    texto_cortado(pintor, pos2(x, rect.top() + 33.0), valor, forte(14.0), p.texto, largura);
    if let Some(sub) = sub {
        texto_cortado(pintor, pos2(x, rect.top() + 50.0), sub, FontId::proportional(11.5), p.suave, largura);
    }
}

/// O cabeçalho do dia na lista: o nome, um fio e quantas concluiu.
fn cabecalho_do_dia(pintor: &egui::Painter, rect: Rect, dia: &DiaConcluidas, busca: &str) {
    let p = cores();
    let meio = rect.center().y + 3.0;
    let titulo = pintor.layout_no_wrap(dia.titulo.clone(), forte(12.5), p.texto);
    let largura_titulo = titulo.size().x;
    pintor.galley(pos2(rect.left() + 8.0, meio - titulo.size().y / 2.0), titulo, p.texto);
    let n = dia.tarefas.iter().filter(|t| busca.is_empty() || t.busca.contains(busca)).count();
    let numero = pintor.layout_no_wrap(n.to_string(), FontId::proportional(11.5), p.suave);
    let x_numero = rect.right() - 8.0 - numero.size().x;
    pintor.galley(pos2(x_numero, meio - numero.size().y / 2.0), numero, p.suave);
    let inicio = rect.left() + 8.0 + largura_titulo + 10.0;
    if x_numero - 8.0 > inicio {
        pintor.line_segment([pos2(inicio, meio), pos2(x_numero - 8.0, meio)], Stroke::new(1.0, p.borda.gamma_multiply(0.7)));
    }
}

/// Uma tarefa na lista: o visto, o título, o número (e o projeto) e a hora.
fn linha_da_tarefa(ui: &mut egui::Ui, rect: Rect, t: &Concluida, escolhida: bool, um_projeto: bool) -> egui::Response {
    let p = cores();
    let resposta = ui.interact(rect, Id::new(("concluida", t.tarefa)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    if escolhida {
        ui.painter().rect(rect, CornerRadius::same(12), p.destaque.gamma_multiply(0.13), Stroke::new(1.0, p.destaque.gamma_multiply(0.5)), StrokeKind::Inside);
    } else {
        tema::fundo_ao_passar(ui, &resposta, rect, 12, true);
    }
    let pintor = ui.painter();
    let meio = rect.center().y;
    let visto = pos2(rect.left() + 21.0, meio);
    pintor.circle_filled(visto, 11.0, p.destaque.gamma_multiply(if escolhida { 0.22 } else { 0.12 }));
    tema::desenhar_icone(pintor, visto, Icone::Check, p.destaque);
    let hora = pintor.layout_no_wrap(t.hora.clone(), FontId::proportional(12.0), p.suave);
    let x_hora = rect.right() - 10.0 - hora.size().x;
    pintor.galley(pos2(x_hora, meio - hora.size().y / 2.0), hora, p.suave);
    let x = rect.left() + 42.0;
    let largura = x_hora - 10.0 - x;
    let fonte = if escolhida { forte(13.5) } else { FontId::proportional(13.5) };
    let cortou = texto_cortado(pintor, pos2(x, meio - 9.0), &t.titulo, fonte, p.texto, largura);
    let sub = if um_projeto || t.projeto.is_empty() { format!("#{}", t.tarefa) } else { format!("#{} · {}", t.tarefa, t.projeto) };
    texto_cortado(pintor, pos2(x, meio + 10.0), &sub, FontId::proportional(11.5), p.suave, largura);
    if cortou { resposta.on_hover_text(&t.titulo) } else { resposta }
}

/// A lupa da busca, desenhada.
fn lupa(pintor: &egui::Painter, c: egui::Pos2, cor: Color32) {
    pintor.circle_stroke(c + vec2(-1.5, -1.5), 4.5, Stroke::new(1.5, cor));
    pintor.line_segment([c + vec2(1.8, 1.8), c + vec2(5.0, 5.0)], Stroke::new(1.5, cor));
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(tipo: &str, tarefa: i64, coluna: &str) -> serde_json::Value {
        serde_json::json!({"evento": 1, "hora": "10:00", "tipo": tipo, "texto": "x", "tarefa_id": tarefa, "titulo": format!("Tarefa {tarefa}"), "coluna": coluna})
    }

    #[test]
    fn concluidas_por_dia_so_a_mais_recente_e_so_as_que_continuam_concluidas() {
        let dias: Vec<api::Dia> = serde_json::from_value(serde_json::json!([
            {"dia": "2026-10-16", "titulo": "Hoje", "itens": [item("concluiu", 1, "concluido"), item("sessao", 2, "concluido"), item("concluiu", 3, "backlog")]},
            {"dia": "2026-10-15", "titulo": "Ontem", "itens": [item("concluiu", 1, "concluido"), item("concluiu", 2, "concluido")]},
            {"dia": "2026-10-14", "titulo": "Quarta, 14/10", "itens": [item("moveu", 4, "concluido")]}
        ]))
        .unwrap();
        let c = concluidas(&dias);
        let resumo: Vec<(&str, Vec<i64>)> = c.iter().map(|d| (d.titulo.as_str(), d.tarefas.iter().map(|t| t.tarefa).collect())).collect();
        // A 1 concluiu de novo hoje: aparece só hoje. A 3 foi reaberta: sai.
        assert_eq!(resumo, vec![("Hoje", vec![1]), ("Ontem", vec![2])]);
        assert_eq!(c[0].tarefas[0].busca, "tarefa 1");
        assert_eq!(dia_curto("Ontem"), "ontem");
        assert_eq!(dia_curto("Hoje · sexta, 16 de outubro"), "hoje");
    }
}
