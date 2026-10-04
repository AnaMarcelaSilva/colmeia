//! Página da linha do tempo: um cabeçalho por dia (fixo ao rolar), um
//! cartão por tarefa dentro do dia e um bloco discreto para o que não é de
//! tarefa (projeto adicionado, por exemplo). As repetições seguidas viram
//! uma linha com "(N vezes)". A lista é virtualizada com alturas calculadas
//! sem montar texto.

use eframe::egui::{self, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::comum::{self, desenhar_miniatura, texto_cortado};
use super::{Acao, Filtro, Registro};
use crate::api;
use crate::tema::{self, Pilula, cores, forte};

const CABECALHO: f32 = 48.0;
const MARGEM_CARTAO: f32 = 12.0;
const LINHA_TITULO: f32 = 22.0;
const EVENTO: f32 = 22.0;
const MINIATURAS: f32 = 10.0 + 64.0;
const ENTRE_CARTOES: f32 = 8.0;
const ENTRE_DIAS: f32 = 20.0;
const LINHA_PERFIL: f32 = 20.0;
const LINHA_ESPECIAL: f32 = 48.0;
/// Eventos por cartão antes do "Ver mais".
const EVENTOS_VISIVEIS: usize = 4;
const MAX_MINIATURAS: usize = 6;

/// Um evento do cartão (as repetições seguidas juntas).
struct Evento<'a> {
    hora: &'a str,
    tipo: &'a str,
    texto: &'a str,
    vezes: usize,
}

impl Evento<'_> {
    /// A frase com a contagem dentro: "Anotou na daily (2 vezes)."
    fn frase(&self) -> std::borrow::Cow<'_, str> {
        if self.vezes > 1 { format!("{} ({} vezes).", self.texto.trim_end_matches('.'), self.vezes).into() } else { self.texto.into() }
    }
}

/// Uma miniatura do cartão, com a legenda e a hora do item de onde veio
/// (o visor mostra as do anexo clicado, não as do cartão).
struct AnexoCartao<'a> {
    id: i64,
    video: bool,
    texto: &'a str,
    hora: &'a str,
}

struct Cartao<'a> {
    tarefa: i64,
    agente: i64,
    titulo: &'a str,
    projeto: &'a str,
    coluna: &'a str,
    removida: bool,
    erro: bool,
    eventos: Vec<Evento<'a>>,
    anexos: Vec<AnexoCartao<'a>>,
}

struct DiaVisual<'a> {
    dia: &'a api::Dia,
    cartoes: Vec<Cartao<'a>>,
    perfil: Vec<&'a api::ItemLinha>,
}

impl Cartao<'_> {
    fn altura(&self, expandido: bool) -> f32 {
        let n = if expandido { self.eventos.len() } else { self.eventos.len().min(EVENTOS_VISIVEIS) };
        let mais = if !expandido && self.eventos.len() > EVENTOS_VISIVEIS { EVENTO } else { 0.0 };
        let imagens = if self.anexos.is_empty() { 0.0 } else { MINIATURAS };
        MARGEM_CARTAO * 2.0 + LINHA_TITULO + 6.0 + n as f32 * EVENTO + mais + imagens
    }

    fn passa(&self, filtro: Option<Filtro>) -> bool {
        match filtro {
            None => true,
            Some(Filtro::Conclusoes) => self.eventos.iter().any(|e| e.tipo == "concluiu"),
            Some(Filtro::Erros) => self.erro,
            Some(Filtro::Capturas) => !self.anexos.is_empty(),
        }
    }
}

/// Agrupa os itens de um dia por tarefa, na ordem do evento mais recente.
fn agrupar<'a>(dia: &'a api::Dia, filtro: Option<Filtro>) -> DiaVisual<'a> {
    let mut cartoes: Vec<Cartao<'a>> = Vec::new();
    let mut perfil = Vec::new();
    for item in &dia.itens {
        if item.tarefa_id == 0 {
            if item.tipo == "captura" {
                continue; // imagem sem tarefa: aparece só na galeria da sprint
            }
            perfil.push(item);
            continue;
        }
        let i = match cartoes.iter().position(|c| c.tarefa == item.tarefa_id) {
            Some(i) => i,
            None => {
                cartoes.push(Cartao {
                    tarefa: item.tarefa_id,
                    agente: item.agente_id,
                    titulo: if item.titulo.is_empty() { "tarefa" } else { &item.titulo },
                    projeto: &item.projeto,
                    coluna: &item.coluna,
                    removida: item.removida,
                    erro: false,
                    eventos: Vec::new(),
                    anexos: Vec::new(),
                });
                cartoes.len() - 1
            }
        };
        let c = &mut cartoes[i];
        c.erro |= item.tipo == "erro";
        for &id in &item.anexos {
            c.anexos.push(AnexoCartao { id, video: item.videos.contains(&id), texto: &item.texto, hora: &item.hora });
        }
        let texto = if item.curto.is_empty() { &item.texto } else { &item.curto };
        match c.eventos.last_mut() {
            Some(e) if e.texto == texto && e.tipo == item.tipo => e.vezes += 1,
            _ => c.eventos.push(Evento { hora: &item.hora, tipo: &item.tipo, texto, vezes: 1 }),
        }
    }
    cartoes.retain(|c| c.passa(filtro));
    if filtro.is_some() {
        perfil.clear();
    }
    DiaVisual { dia, cartoes, perfil }
}

enum Fileira {
    Cabecalho(usize),
    Cartao(usize, usize),
    Perfil(usize),
    Carregando,
    Inicio,
    SemResultado,
}

impl Registro {
    pub(super) fn pagina_linha(&mut self, ui: &mut egui::Ui, projeto: Option<i64>, acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let titulo = if projeto.is_some() { "Linha do tempo do projeto" } else { "Linha do tempo" };
        tema::cabecalho(ui, titulo, "Tudo o que aconteceu, dia a dia, agrupado por tarefa.");
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (rotulo, filtro) in [("Só conclusões", Filtro::Conclusoes), ("Só erros", Filtro::Erros), ("Com capturas", Filtro::Capturas)] {
                let ativo = self.filtro == Some(filtro);
                if tema::chip_alternar(ui, rotulo, ativo).clicked() {
                    self.filtro = if ativo { None } else { Some(filtro) };
                }
            }
        });
        ui.add_space(12.0);

        if let Some(e) = self.erro.clone()
            && !api::erro_de_conexao(&e)
        {
            if comum::faixa_erro(ui, &format!("Não consegui carregar a linha do tempo: {e}")) {
                self.pedir_pagina(&ctx, true);
            }
            ui.add_space(12.0);
            if self.dias.is_empty() {
                return;
            }
        }
        if self.carregou && self.dias.is_empty() {
            let (titulo, texto, botao) = match projeto {
                Some(_) => ("Nada neste projeto ainda", "Crie uma tarefa no Quadro e a história dela aparece nesta página.", "Ver todos os projetos"),
                None => ("Ainda não aconteceu nada aqui.", "Crie uma tarefa no Quadro e a história dela aparece nesta página.", "Ir ao Quadro"),
            };
            if comum::vazio(ui, titulo, texto, Some(botao)) {
                acoes.push(if projeto.is_some() { Acao::VerTodos } else { Acao::IrParaQuadro });
            }
            return;
        }

        let dias: Vec<DiaVisual> = self.dias.iter().map(|d| agrupar(d, self.filtro)).collect();
        let com_projeto = projeto.is_none();
        // Fileiras com alturas fixas, calculadas sem montar texto.
        let mut fileiras: Vec<(f32, f32, Fileira)> = Vec::new();
        let mut y = 0.0;
        for (d, dia) in dias.iter().enumerate() {
            if dia.cartoes.is_empty() && dia.perfil.is_empty() {
                continue;
            }
            fileiras.push((y, CABECALHO, Fileira::Cabecalho(d)));
            y += CABECALHO;
            for (c, cartao) in dia.cartoes.iter().enumerate() {
                let h = cartao.altura(self.expandidos.contains(&(dia.dia.dia.clone(), cartao.tarefa)));
                fileiras.push((y, h, Fileira::Cartao(d, c)));
                y += h + ENTRE_CARTOES;
            }
            if !dia.perfil.is_empty() {
                let h = LINHA_PERFIL * (dia.perfil.len() + 1) as f32 + 6.0;
                fileiras.push((y, h, Fileira::Perfil(d)));
                y += h;
            }
            y += ENTRE_DIAS;
        }
        if fileiras.is_empty() && self.carregou {
            fileiras.push((y, LINHA_ESPECIAL * 2.0, Fileira::SemResultado));
            y += LINHA_ESPECIAL * 2.0;
        }
        if self.proximo > 0 && self.filtro.is_none() {
            fileiras.push((y, EVENTO + 14.0, Fileira::Carregando));
            y += EVENTO + 14.0;
        } else if self.carregou && self.proximo == 0 {
            fileiras.push((y, LINHA_ESPECIAL, Fileira::Inicio));
            y += LINHA_ESPECIAL;
        }
        let total = y;

        let mut rolagem = egui::ScrollArea::vertical().id_salt("linha-do-tempo").auto_shrink(false);
        if std::mem::take(&mut self.ir_ao_topo) {
            rolagem = rolagem.vertical_scroll_offset(0.0);
        }
        let mut pedir_mais = false;
        let mut expandir = None;
        let mut visor = None;
        let saida = rolagem.show_viewport(ui, |ui, janela| {
            ui.set_height(total);
            let origem = ui.max_rect().min;
            // A barra de rolagem fica numa faixa só dela, sem nada por baixo.
            let largura = ui.available_width() - comum::MARGEM_ROLAGEM;
            // O cabeçalho preso é o do último dia que já começou acima do topo;
            // o do dia seguinte, ao chegar, empurra o preso para cima (nunca
            // passa por baixo dele), para os cartões nunca aparecerem sob o
            // dia errado.
            let mut cabecalho_fixo = None;
            let mut proximo_cabecalho = f32::INFINITY;
            for (fy, _, fileira) in &fileiras {
                if let Fileira::Cabecalho(d) = fileira {
                    if *fy <= janela.min.y {
                        cabecalho_fixo = Some(*d);
                    } else {
                        proximo_cabecalho = *fy;
                        break;
                    }
                }
            }
            // Retângulo do cabeçalho preso; as fileiras são recortadas abaixo
            // dele, para o cartão escondido não responder ao mouse.
            let preso = cabecalho_fixo.filter(|_| janela.min.y > 0.0).map(|d| {
                let topo = janela.min.y.min(proximo_cabecalho - CABECALHO);
                (d, Rect::from_min_size(origem + vec2(0.0, topo), vec2(largura, CABECALHO)))
            });
            let recorte = ui.clip_rect();
            if let Some((_, rect)) = preso {
                let mut abaixo = recorte;
                abaixo.min.y = abaixo.min.y.max(rect.max.y);
                ui.set_clip_rect(abaixo);
            }
            for (fy, fh, fileira) in &fileiras {
                if fy + fh < janela.min.y || *fy > janela.max.y {
                    continue;
                }
                let rect = Rect::from_min_size(origem + vec2(0.0, *fy), vec2(largura, *fh));
                match fileira {
                    Fileira::Cabecalho(d) => cabecalho_do_dia(ui.painter(), rect, &dias[*d], false),
                    Fileira::Cartao(d, c) => {
                        let dia = &dias[*d];
                        let cartao = &dia.cartoes[*c];
                        let expandido = self.expandidos.contains(&(dia.dia.dia.clone(), cartao.tarefa));
                        match desenhar_cartao(ui, rect, cartao, expandido, com_projeto, &mut self.miniaturas) {
                            Some(Clique::Abrir) => acoes.push(Acao::AbrirTarefa {
                                tarefa: cartao.tarefa,
                                agente: (cartao.agente != 0).then_some(cartao.agente),
                                lousa: cartao.eventos.first().is_some_and(|e| e.tipo == "lousa"),
                            }),
                            Some(Clique::VerMais) => expandir = Some((dia.dia.dia.clone(), cartao.tarefa)),
                            Some(Clique::Imagem(i)) => {
                                let a = &cartao.anexos[i];
                                visor = Some((a.id, a.texto.to_string(), a.hora.to_string(), cartao.tarefa));
                            }
                            None => {}
                        }
                    }
                    Fileira::Perfil(d) => {
                        if let Some(c) = bloco_perfil(ui, rect, &dias[*d].perfil) {
                            acoes.push(Acao::AbrirBanco(c));
                        }
                    }
                    Fileira::Carregando => {
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Carregando dias anteriores…", FontId::proportional(12.5), p.suave);
                        pedir_mais = true;
                    }
                    Fileira::Inicio => {
                        let texto = format!("Início do histórico deste perfil (criado em {}).", self.criado_em);
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, texto, FontId::proportional(12.5), p.suave);
                    }
                    Fileira::SemResultado => {
                        ui.painter().text(
                            rect.center_top() + vec2(0.0, 24.0),
                            egui::Align2::CENTER_CENTER,
                            "Nada com esse filtro nestes dias.",
                            FontId::proportional(13.5),
                            p.suave,
                        );
                        if self.proximo > 0 {
                            let botao = Rect::from_center_size(rect.center_top() + vec2(0.0, 64.0), vec2(220.0, 32.0));
                            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(botao).layout(egui::Layout::top_down(egui::Align::Center)));
                            if tema::botao_secundario(&mut filho, "Carregar dias anteriores").clicked() {
                                pedir_mais = true;
                            }
                        }
                    }
                }
            }
            // O cabeçalho do dia que está passando fica preso no topo, com sombra na base.
            ui.set_clip_rect(recorte);
            if let Some((d, rect)) = preso {
                cabecalho_do_dia(ui.painter(), rect, &dias[d], true);
            }
        });
        if let Some(chave) = expandir {
            self.expandidos.insert(chave);
        }
        if let Some((anexo, texto, hora, tarefa)) = visor {
            self.abrir_visor(&ctx, anexo, &texto, &hora, tarefa);
        }
        self.no_topo = saida.state.offset.y < 4.0;
        if self.no_topo && self.novidades > 0 {
            self.novidades = 0;
            self.pedir_pagina(&ctx, true);
        }
        if pedir_mais && !self.carregando {
            self.pedir_pagina(&ctx, false);
        }
        // Pílula de novidades: estática, some ao clicar ou ao voltar ao topo.
        if self.novidades > 0 {
            let texto = if self.novidades == 1 { "1 novidade · mostrar".to_string() } else { format!("{} novidades · mostrar", self.novidades) };
            let pos = saida.inner_rect.center_top() + vec2(0.0, CABECALHO + 10.0);
            egui::Area::new(Id::new("pilula-novidades")).order(egui::Order::Foreground).pivot(egui::Align2::CENTER_TOP).fixed_pos(pos).show(&ctx, |ui| {
                egui::Frame::new().shadow(tema::sombra(4, 12)).corner_radius(17).show(ui, |ui| {
                    if tema::botao_principal(ui, &texto, true).clicked() {
                        self.ir_ao_topo = true;
                        self.novidades = 0;
                        self.pedir_pagina(&ctx, true);
                    }
                });
            });
        }
    }
}

/// Cabeçalho do dia: título à esquerda e os números à direita (zeros não
/// aparecem): o que aconteceu no dia, com os mesmos nomes e contas da Daily
/// (concluídas, erros de sessão, tempo de agente). Preso no topo, ganha fundo
/// opaco e sombra na base.
fn cabecalho_do_dia(pintor: &egui::Painter, rect: Rect, dia: &DiaVisual, preso: bool) {
    let p = cores();
    if preso {
        let sombra = Rect::from_min_size(rect.left_bottom(), vec2(rect.width(), 8.0));
        for i in 0..4 {
            let faixa = Rect::from_min_size(sombra.min + vec2(0.0, i as f32 * 2.0), vec2(rect.width(), 2.0));
            pintor.rect_filled(faixa, 0, egui::Color32::from_black_alpha(if tema::claro() { 10 } else { 26 } / (i + 1)));
        }
        pintor.rect_filled(rect, 0, p.fundo);
    }
    let meio = rect.center().y;
    pintor.text(pos2(rect.left() + 2.0, meio), egui::Align2::LEFT_CENTER, &dia.dia.titulo, forte(15.0), p.texto);
    let mut partes: Vec<(String, Option<(egui::Color32, tema::Marca)>)> = Vec::new();
    let plural = |n: usize, um: &str, varios: &str| if n == 1 { format!("1 {um}") } else { format!("{n} {varios}") };
    if dia.dia.concluidas > 0 {
        partes.push((plural(dia.dia.concluidas, "concluída", "concluídas"), Some((p.destaque, tema::Marca::Cheia))));
    }
    if dia.dia.erros > 0 {
        partes.push((plural(dia.dia.erros, "erro", "erros"), Some((p.erro, tema::Marca::AnelGrosso))));
    }
    if dia.dia.tempo_s >= 60 {
        partes.push((format!("{} de agente", comum::duracao(dia.dia.tempo_s)), None));
    }
    let mut x = rect.right() - 2.0;
    for (texto, cor) in partes.iter().rev() {
        let galeria = pintor.layout_no_wrap(texto.clone(), FontId::proportional(12.5), p.texto);
        x -= galeria.size().x;
        let cor_texto = if cor.is_some() { p.texto } else { p.suave };
        pintor.galley(pos2(x, meio - galeria.size().y / 2.0), galeria, cor_texto);
        if let Some((c, marca)) = cor {
            x -= 12.0;
            tema::marca(pintor, pos2(x + 4.0, meio), 4.0, *c, *marca);
        }
        x -= 16.0;
    }
}

enum Clique {
    Abrir,
    VerMais,
    /// O índice da miniatura no cartão.
    Imagem(usize),
}

/// Um cartão de tarefa: título (clicável), projeto e estado na primeira
/// linha; os eventos em lista compacta; as miniaturas no fim.
fn desenhar_cartao(ui: &mut egui::Ui, rect: Rect, c: &Cartao, expandido: bool, com_projeto: bool, cache: &mut comum::CacheImagens) -> Option<Clique> {
    let p = cores();
    let ctx = ui.ctx().clone();
    let mut clique = None;
    let pintor = ui.painter().clone();
    let fundo_cartao = ui.interact(rect, Id::new(("cartao-linha", c.tarefa, rect.min.y as i32)), Sense::hover());
    if c.removida {
        // Tarefa que saiu: sem fundo (o `suave` só passa de 4,5:1 sobre o
        // fundo da página), com borda tracejada.
        tracejado(&pintor, rect.shrink(0.5), p.borda);
    } else {
        let borda = if fundo_cartao.hovered() { p.destaque.gamma_multiply(0.55) } else { p.borda };
        pintor.rect(rect, CornerRadius::same(tema::RAIO_CARTAO), p.superficie_alta, Stroke::new(1.0, borda), StrokeKind::Inside);
    }
    let interno = rect.shrink2(vec2(14.0, MARGEM_CARTAO));
    let linha1 = Rect::from_min_size(interno.min, vec2(interno.width(), LINHA_TITULO));
    let meio = linha1.center().y;

    // À direita: o estado e o projeto.
    let mut direita = linha1.right();
    let (cor, rotulo, marca) = tema::estado_da_tarefa(c.coluna, c.erro, c.removida);
    let pilula =
        if c.removida { Pilula::neutra(rotulo) } else { Pilula { texto: rotulo, cor, cheio: marca == tema::Marca::Cheia, ponto: true, grande: false } };
    direita -= pilula.largura(&pintor);
    pilula.pintar(&pintor, pos2(direita, meio - 11.0));
    if com_projeto && !c.projeto.is_empty() {
        let largura = pintor.layout_no_wrap(c.projeto.to_string(), tema::fonte_etiqueta(), p.suave).size().x + 12.0;
        direita -= largura + 8.0;
        tema::etiqueta(&pintor, pos2(direita, meio - 9.5), c.projeto, tema::fonte_etiqueta(), p.suave);
    }
    let largura_titulo = (direita - 12.0 - linha1.left()).max(40.0);
    let galeria = pintor.layout_no_wrap(c.titulo.to_string(), forte(14.0), p.texto);
    let area_titulo = Rect::from_min_size(linha1.min, vec2(galeria.size().x.min(largura_titulo), LINHA_TITULO));
    let cor_titulo = if c.removida { p.suave } else { p.texto };
    let cortou = texto_cortado(&pintor, pos2(linha1.left(), meio), c.titulo, forte(14.0), cor_titulo, largura_titulo);
    if !c.removida {
        let resposta =
            ui.interact(area_titulo, Id::new(("titulo-cartao", c.tarefa, rect.min.y as i32)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resposta.hovered() {
            pintor.line_segment([pos2(area_titulo.left(), meio + 9.0), pos2(area_titulo.right(), meio + 9.0)], Stroke::new(1.0, p.texto));
        }
        let resposta = if cortou { resposta.on_hover_text(c.titulo) } else { resposta.on_hover_text("Abrir a tarefa") };
        if resposta.clicked() {
            clique = Some(Clique::Abrir);
        }
    }

    // Eventos: hora, ponto e frase.
    let mut y = linha1.bottom() + 6.0;
    let visiveis = if expandido { c.eventos.len() } else { c.eventos.len().min(EVENTOS_VISIVEIS) };
    for e in &c.eventos[..visiveis] {
        let meio = y + EVENTO / 2.0;
        pintor.text(pos2(interno.left(), meio), egui::Align2::LEFT_CENTER, e.hora, FontId::proportional(12.5), p.suave);
        tema::ponto(&pintor, pos2(interno.left() + 50.0, meio), 3.0, comum::ponto_do_tipo(e.tipo));
        let x = interno.left() + 60.0;
        let fim = interno.right();
        let frase = e.frase();
        let cortou = texto_cortado(&pintor, pos2(x, meio), &frase, FontId::proportional(13.5), p.texto, fim - x);
        if cortou {
            ui.interact(Rect::from_min_max(pos2(x, y), pos2(fim, y + EVENTO)), Id::new(("evento", c.tarefa, rect.min.y as i32, y as i32)), Sense::hover())
                .on_hover_text(frase.as_ref());
        }
        y += EVENTO;
    }
    if !expandido && c.eventos.len() > EVENTOS_VISIVEIS {
        let texto = format!("Ver mais {}", c.eventos.len() - EVENTOS_VISIVEIS);
        let galeria = pintor.layout_no_wrap(texto.clone(), FontId::proportional(12.5), p.destaque);
        let area = Rect::from_min_size(pos2(interno.left() + 60.0, y + (EVENTO - galeria.size().y) / 2.0), galeria.size());
        let resposta = ui.interact(area, Id::new(("ver-mais", c.tarefa, rect.min.y as i32)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        pintor.galley(area.min, galeria, p.destaque);
        if resposta.hovered() {
            pintor.line_segment([area.left_bottom(), area.right_bottom()], Stroke::new(1.0, p.destaque));
        }
        if resposta.clicked() {
            clique = Some(Clique::VerMais);
        }
        y += EVENTO;
    }
    if !c.anexos.is_empty() {
        let mut x = interno.left() + 60.0;
        let y = y + 10.0;
        for (n, anexo) in c.anexos.iter().enumerate() {
            let caixa = Rect::from_min_size(pos2(x, y), vec2(64.0, 64.0));
            if n == MAX_MINIATURAS {
                comum::caixa_mais(&pintor, caixa, c.anexos.len() - MAX_MINIATURAS);
                break;
            }
            if anexo.video {
                comum::caixa_video(ui, caixa, Id::new(("video-linha", anexo.id, rect.min.y as i32)), tema::RAIO_CONTROLE)
                    .on_hover_text("Vídeo: abra na apresentação da daily ou da sprint");
            } else if desenhar_miniatura(ui, cache, caixa, anexo.id, tema::RAIO_CONTROLE).on_hover_text(format!("{} · {}", anexo.texto, anexo.hora)).clicked()
            {
                clique = Some(Clique::Imagem(n));
            }
            x += 64.0 + 8.0;
        }
        let _ = ctx;
    }
    clique
}

/// Borda tracejada de 1 px (traço 6, vão 4) num retângulo.
fn tracejado(pintor: &egui::Painter, r: Rect, cor: egui::Color32) {
    let caminho: Vec<Pos2> = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for forma in egui::Shape::dashed_line(&caminho, Stroke::new(1.0, cor), 6.0, 4.0) {
        pintor.add(forma);
    }
}

/// Eventos sem tarefa (projeto adicionado, perfil): uma linha cada, discreta.
/// O bloco do que é do perfil (projetos, conexões de banco). Um item de
/// banco é clicável: abre a tela de bancos nessa conexão.
fn bloco_perfil(ui: &mut egui::Ui, rect: Rect, itens: &[&api::ItemLinha]) -> Option<i64> {
    let p = cores();
    let pintor = ui.painter().clone();
    pintor.text(pos2(rect.left() + 2.0, rect.top() + LINHA_PERFIL / 2.0), egui::Align2::LEFT_CENTER, "Perfil e projetos", forte(12.0), p.suave);
    let mut clicada = None;
    for (i, item) in itens.iter().enumerate() {
        let meio = rect.top() + LINHA_PERFIL * (i as f32 + 1.5) + 6.0;
        let linha = Rect::from_min_max(pos2(rect.left(), meio - LINHA_PERFIL / 2.0), pos2(rect.right(), meio + LINHA_PERFIL / 2.0));
        let mut cor = p.suave;
        if item.conexao_id != 0 {
            let r = ui.interact(linha, egui::Id::new(("item-perfil", item.evento, i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.hovered() {
                cor = p.texto;
            }
            if r.on_hover_text("Abrir nos bancos de dados").clicked() {
                clicada = Some(item.conexao_id);
            }
        }
        pintor.text(pos2(rect.left() + 2.0, meio), egui::Align2::LEFT_CENTER, &item.hora, FontId::proportional(12.5), p.suave);
        texto_cortado(&pintor, pos2(rect.left() + 50.0, meio), &item.texto, FontId::proportional(12.5), cor, rect.width() - 52.0);
    }
    clicada
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(hora: &str, tipo: &str, tarefa: i64, curto: &str) -> api::ItemLinha {
        serde_json::from_value(serde_json::json!({
            "evento": 1, "hora": hora, "tipo": tipo, "texto": format!("{curto} em “T{tarefa}”"), "curto": curto,
            "tarefa_id": tarefa, "titulo": format!("T{tarefa}"), "coluna": "trabalhando"
        }))
        .unwrap()
    }

    #[test]
    fn agrupa_por_tarefa_e_junta_repeticoes() {
        let mut dia: api::Dia = serde_json::from_value(serde_json::json!({"dia": "2026-10-02", "titulo": "Hoje", "itens": []})).unwrap();
        dia.itens = vec![
            item("10:05", "sessao", 1, "Claude Code (dev) trabalhou menos de 1 min."),
            item("10:04", "sessao", 1, "Claude Code (dev) trabalhou menos de 1 min."),
            item("10:03", "erro", 2, "Codex parou com erro."),
            item("10:02", "sessao", 1, "Claude Code (dev) trabalhou menos de 1 min."),
            item("10:01", "concluiu", 1, "Concluída."),
        ];
        let mut sem_tarefa = item("09:00", "projeto", 0, "");
        sem_tarefa.texto = "Adicionou o projeto loja-web.".into();
        dia.itens.push(sem_tarefa);
        let v = agrupar(&dia, None);
        assert_eq!(v.cartoes.len(), 2);
        assert_eq!(v.cartoes[0].tarefa, 1);
        // Dentro do cartão, o evento de outra tarefa no meio não separa as repetições.
        let eventos: Vec<(usize, &str)> = v.cartoes[0].eventos.iter().map(|e| (e.vezes, e.texto)).collect();
        assert_eq!(eventos, vec![(3, "Claude Code (dev) trabalhou menos de 1 min."), (1, "Concluída.")]);
        assert_eq!(v.perfil.len(), 1);
        assert_eq!(v.cartoes[0].eventos[0].frase(), "Claude Code (dev) trabalhou menos de 1 min (3 vezes).");
        assert!(v.cartoes[1].erro);

        let so_erros = agrupar(&dia, Some(Filtro::Erros));
        assert_eq!(so_erros.cartoes.len(), 1);
        assert!(so_erros.perfil.is_empty());
        assert_eq!(agrupar(&dia, Some(Filtro::Conclusoes)).cartoes[0].tarefa, 1);
        assert!(agrupar(&dia, Some(Filtro::Capturas)).cartoes.is_empty());
    }

    #[test]
    fn altura_do_cartao_segue_os_eventos() {
        let mut dia: api::Dia = serde_json::from_value(serde_json::json!({"dia": "2026-10-02", "titulo": "Hoje", "itens": []})).unwrap();
        dia.itens = (0..6).map(|i| item("10:00", "moveu", 1, &format!("Foi para {i}."))).collect();
        let v = agrupar(&dia, None);
        let c = &v.cartoes[0];
        assert_eq!(c.altura(false), MARGEM_CARTAO * 2.0 + LINHA_TITULO + 6.0 + 4.0 * EVENTO + EVENTO);
        assert_eq!(c.altura(true), MARGEM_CARTAO * 2.0 + LINHA_TITULO + 6.0 + 6.0 * EVENTO);
    }
}
