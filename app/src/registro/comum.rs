//! Peças comuns da linha do tempo, da daily, da sprint e da apresentação:
//! pedidos em segundo plano, o cache de imagens, a miniatura e o cartão de
//! tarefa da Daily e da Sprint.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, ColorImage, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, pos2, vec2};

use crate::api;
use crate::tema::{self, EstadoVisual, Marca, Pilula, cores, forte};

/// Faz o pedido numa thread e acorda a tela quando a resposta chega.
pub fn em_segundo_plano<T: Send + 'static>(envio: &Sender<T>, ctx: &egui::Context, pedido: impl FnOnce() -> T + Send + 'static) {
    let envio = envio.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = envio.send(pedido());
        ctx.request_repaint();
    });
}

/// "25/09/2026" vira "2026-09-25"; outra coisa é recusada aqui mesmo.
pub fn data_da_tela(texto: &str) -> Option<String> {
    let partes: Vec<&str> = texto.trim().split('/').collect();
    match partes[..] {
        [d, m, a] if d.len() == 2 && m.len() == 2 && a.len() == 4 && texto.trim().chars().all(|c| c.is_ascii_digit() || c == '/') => {
            Some(format!("{a}-{m}-{d}"))
        }
        _ => None,
    }
}

/// Duração em português curto, como o núcleo escreve: "1h05", "12 min".
pub fn duracao(segundos: i64) -> String {
    match segundos {
        s if s < 60 => "menos de 1 min".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{}h{:02}", s / 3600, (s % 3600) / 60),
    }
}

/// Tamanho de arquivo: "12,3 MB".
pub fn tamanho(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64).replace('.', ","),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64).replace('.', ","),
        b if b >= 1 << 10 => format!("{} KB", b >> 10),
        b => format!("{b} B"),
    }
}

/// Decodifica um PNG e, com `altura_maxima`, reduz a altura (média de cada bloco).
pub fn decodificar(png: &[u8], altura_maxima: Option<u32>) -> Result<ColorImage, String> {
    let mut decodificador = png::Decoder::new(std::io::Cursor::new(png));
    decodificador.set_transformations(png::Transformations::normalize_to_color8());
    let mut leitor = decodificador.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; leitor.output_buffer_size().ok_or("imagem grande demais")?];
    let info = leitor.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    let (largura, altura) = (info.width as usize, info.height as usize);
    let canais = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("paleta não expandida".into()),
    };
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let i = y * info.line_size + x * canais;
        match canais {
            4 => [buffer[i], buffer[i + 1], buffer[i + 2], buffer[i + 3]],
            3 => [buffer[i], buffer[i + 1], buffer[i + 2], 255],
            2 => [buffer[i], buffer[i], buffer[i], buffer[i + 1]],
            _ => [buffer[i], buffer[i], buffer[i], 255],
        }
    };
    let fator = altura_maxima.map_or(1, |m| altura.div_ceil(m as usize).max(1));
    let (nl, na) = ((largura / fator).max(1), (altura / fator).max(1));
    let mut rgba = Vec::with_capacity(nl * na * 4);
    for y in 0..na {
        for x in 0..nl {
            let mut soma = [0u32; 4];
            let mut n = 0;
            for dy in 0..fator {
                for dx in 0..fator {
                    let (sx, sy) = (x * fator + dx, y * fator + dy);
                    if sx < largura && sy < altura {
                        for (s, v) in soma.iter_mut().zip(pixel(sx, sy)) {
                            *s += v as u32;
                        }
                        n += 1;
                    }
                }
            }
            rgba.extend(soma.map(|s| (s / n.max(1)) as u8));
        }
    }
    Ok(ColorImage::from_rgba_unmultiplied([nl, na], &rgba))
}

/// Tipo de um item da linha do tempo → o ponto da tabela de estados.
pub fn ponto_do_tipo(tipo: &str) -> EstadoVisual {
    match tipo {
        "concluiu" => EstadoVisual::Concluiu,
        "erro" => EstadoVisual::Erro,
        "sessao_aberta" => EstadoVisual::Trabalhando,
        "sessao_aguardando" => EstadoVisual::SuaVez,
        // Terminal aberto e parado: anel cinza, como "Parado" no cartão.
        "sessao_parada" => EstadoVisual::Parado,
        "interrompido" => EstadoVisual::Interrompido,
        _ => EstadoVisual::Terminou,
    }
}

/// Uma linha de texto cortada com "…" (o centro vertical em `pos.y`); diz se cortou.
pub fn texto_cortado(pintor: &egui::Painter, pos: Pos2, texto: &str, fonte: FontId, cor: Color32, largura: f32) -> bool {
    let galeria = tema::cortar(pintor, texto, egui::TextFormat::simple(fonte, cor), largura, 1, true);
    let cortou = galeria.elided;
    pintor.galley(pos - vec2(0.0, galeria.size().y / 2.0), galeria, cor);
    cortou
}

/// Texto em até `linhas` linhas, cortado com "…" no fim.
pub fn texto_em_linhas(pintor: &egui::Painter, texto: &str, fonte: FontId, cor: Color32, largura: f32, linhas: usize) -> std::sync::Arc<egui::Galley> {
    tema::cortar(pintor, texto, egui::TextFormat::simple(fonte, cor), largura, linhas, false)
}

/// Cor, rótulo e marca do estado de um slide (a coluna da tarefa, ou erro).
pub fn estado_do_slide(s: &api::Slide) -> (Color32, &'static str, Marca) {
    tema::estado_da_tarefa(&s.coluna, s.grupo == "erros", s.removida)
}

/// Grupos da Daily, na ordem do deck: chave, título, cor e marca (as mesmas
/// das pílulas e dos números).
pub fn grupos() -> [(&'static str, &'static str, Color32, Marca); 6] {
    let p = cores();
    [
        ("concluidas", "Concluídas", p.destaque, Marca::Cheia),
        ("revisao", "Em revisão", p.texto, Marca::Cheia),
        ("aguardando", "Aguardando você", p.alerta, Marca::Anel),
        ("trabalhando", "Trabalhando", p.ok, Marca::Cheia),
        ("erros", "Com erro", p.erro, Marca::AnelGrosso),
        ("outras", "Outras", p.suave, Marca::Anel),
    ]
}

/// Um número da fileira: valor, rótulo e a marca do estado (o tempo não tem).
type Numero = (String, &'static str, Option<(Color32, Marca)>);

/// Os números do período, na ordem e com os nomes dos grupos (zeros
/// escondidos, menos Concluídas e o tempo).
pub fn numeros_visiveis(n: &api::NumerosCapa) -> Vec<Numero> {
    let p = cores();
    let mut lista = vec![(n.concluidas.to_string(), "Concluídas", Some((p.destaque, Marca::Cheia)))];
    for (valor, rotulo, cor, marca) in [
        (n.revisao, "Em revisão", p.texto, Marca::Cheia),
        (n.aguardando, "Aguardando você", p.alerta, Marca::Anel),
        (n.trabalhando, "Trabalhando", p.ok, Marca::Cheia),
        (n.erros, "Erros", p.erro, Marca::AnelGrosso),
    ] {
        if valor > 0 {
            lista.push((valor.to_string(), rotulo, Some((cor, marca))));
        }
    }
    lista.push((if n.tempo_s < 60 { "0".to_string() } else { duracao(n.tempo_s) }, "Tempo de agente", None));
    lista
}

/// "1 tarefa nova", "3 tarefas novas" (vazio se nenhuma).
pub fn texto_novas(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => "1 tarefa nova".into(),
        n => format!("{n} tarefas novas"),
    }
}

/// Largura da fileira de números, para centralizar ou decidir a quebra.
pub fn largura_numeros(pintor: &egui::Painter, n: &api::NumerosCapa, tamanho: f32, com_novas: bool) -> f32 {
    let p = cores();
    let rotulo = if tamanho >= 48.0 { 16.0 } else { 13.0 };
    let vao = if tamanho >= 48.0 { 56.0 } else { 40.0 };
    let itens = numeros_visiveis(n);
    let soma: f32 = itens
        .iter()
        .map(|(valor, nome, ponto)| {
            let v = pintor.layout_no_wrap(valor.clone(), forte(tamanho), p.texto).size().x;
            let r = pintor.layout_no_wrap((*nome).to_string(), FontId::proportional(rotulo), p.suave).size().x + if ponto.is_some() { 14.0 } else { 0.0 };
            v.max(r)
        })
        .sum();
    let novas = if com_novas { texto_novas(n.novas) } else { String::new() };
    let extra = if novas.is_empty() { 0.0 } else { vao + pintor.layout_no_wrap(novas, FontId::proportional(13.5), p.suave).size().x };
    soma + vao * (itens.len() as f32 - 1.0) + extra
}

/// Uma imagem que chegou da thread: o anexo e a imagem decodificada.
type ImagemLida = (i64, Result<ColorImage, String>);

pub enum Miniatura {
    Carregando,
    Pronta(TextureHandle),
    Falhou,
}

/// Imagens dos anexos, buscadas e decodificadas fora da thread da tela, num
/// cache limitado (as menos usadas saem primeiro).
pub struct CacheImagens {
    altura: Option<u32>,
    maximo: usize,
    prefixo: &'static str,
    mapa: HashMap<i64, Miniatura>,
    uso: VecDeque<i64>,
    canal: (Sender<ImagemLida>, Receiver<ImagemLida>),
}

impl CacheImagens {
    /// `altura`: a altura guardada (o dobro do que aparece, para telas densas); None guarda inteira.
    pub fn new(altura: Option<u32>, maximo: usize, prefixo: &'static str) -> Self {
        CacheImagens { altura, maximo, prefixo, mapa: HashMap::new(), uso: VecDeque::new(), canal: mpsc::channel() }
    }

    /// A imagem do cache; pede se ainda não veio.
    pub fn pedir(&mut self, ctx: &egui::Context, id: i64) -> &Miniatura {
        if let std::collections::hash_map::Entry::Vacant(e) = self.mapa.entry(id) {
            e.insert(Miniatura::Carregando);
            let altura = self.altura;
            em_segundo_plano(&self.canal.0, ctx, move || (id, api::ler_anexo(id).and_then(|png| decodificar(&png, altura))));
        }
        if self.uso.back() != Some(&id) {
            self.uso.retain(|x| *x != id);
            self.uso.push_back(id);
        }
        while self.uso.len() > self.maximo {
            if let Some(velha) = self.uso.pop_front() {
                self.mapa.remove(&velha);
            }
        }
        &self.mapa[&id]
    }

    pub fn ver(&self, id: i64) -> Option<&Miniatura> {
        self.mapa.get(&id)
    }

    pub fn esquecer(&mut self, id: i64) {
        self.mapa.remove(&id);
        self.uso.retain(|x| *x != id);
    }

    /// Recebe as imagens que chegaram e cria as texturas.
    pub fn receber(&mut self, ctx: &egui::Context) {
        while let Ok((id, resultado)) = self.canal.1.try_recv() {
            if self.mapa.contains_key(&id) {
                let m = match resultado {
                    Ok(img) => Miniatura::Pronta(ctx.load_texture(format!("{}-{id}", self.prefixo), img, TextureOptions::LINEAR)),
                    Err(_) => Miniatura::Falhou,
                };
                self.mapa.insert(id, m);
            }
        }
    }
}

/// Desenha a imagem preenchendo a caixa (cortando o excesso, sem distorcer)
/// ou o lugar dela, e devolve a resposta do clique.
pub fn desenhar_miniatura(ui: &mut egui::Ui, cache: &mut CacheImagens, caixa: Rect, anexo: i64, raio: u8) -> egui::Response {
    let p = cores();
    let ctx = ui.ctx().clone();
    let resposta = ui.interact(caixa, Id::new(("miniatura", anexo, caixa.min.x as i32, caixa.min.y as i32)), Sense::click());
    let raio = CornerRadius::same(raio);
    match cache.pedir(&ctx, anexo) {
        Miniatura::Pronta(t) => {
            let tamanho = t.size_vec2();
            let escala = (caixa.width() / tamanho.x).max(caixa.height() / tamanho.y);
            let visivel = vec2(caixa.width() / (tamanho.x * escala), caixa.height() / (tamanho.y * escala));
            egui::Image::new(t).uv(Rect::from_center_size(pos2(0.5, 0.5), visivel)).corner_radius(raio).paint_at(ui, caixa);
        }
        // Sem spinner: um retângulo parado não pede redesenho.
        Miniatura::Falhou => {
            ui.painter().rect_filled(caixa, raio, p.realce);
            ui.painter().text(caixa.center(), egui::Align2::CENTER_CENTER, "indisponível", FontId::proportional(11.0), p.suave);
        }
        Miniatura::Carregando => {
            ui.painter().rect_filled(caixa, raio, p.realce);
        }
    }
    ui.painter().rect_stroke(caixa, raio, Stroke::new(1.0, p.borda), StrokeKind::Inside);
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Caixa de um vídeo (sem miniatura: o app não decodifica vídeo): fundo
/// recuado e o play num círculo de destaque.
pub fn caixa_video(ui: &mut egui::Ui, caixa: Rect, id: Id, raio: u8) -> egui::Response {
    let p = cores();
    let resposta = ui.interact(caixa, id, Sense::click());
    ui.painter().rect(caixa, CornerRadius::same(raio), p.superficie, Stroke::new(1.0, p.borda), StrokeKind::Inside);
    let r = (caixa.height().min(caixa.width()) * 0.19).clamp(10.0, 28.0);
    ui.painter().circle_filled(caixa.center(), r, p.destaque);
    tema::play(ui.painter(), caixa.center(), r * 0.42, tema::sobre_destaque());
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "+N" no fim de uma fileira de miniaturas.
pub fn caixa_mais(pintor: &egui::Painter, caixa: Rect, n: usize) {
    let p = cores();
    pintor.rect_filled(caixa, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
    pintor.text(caixa.center(), egui::Align2::CENTER_CENTER, format!("+{n}"), forte(13.0), p.suave);
}

/// Faixa de erro no topo de uma página, com "Tentar de novo". Diz se clicou.
pub fn faixa_erro(ui: &mut egui::Ui, texto: &str) -> bool {
    let p = cores();
    let mut tentar = false;
    egui::Frame::new()
        .fill(tema::fundo_tingido(p, p.erro, tema::claro()))
        .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
        .inner_margin(egui::Margin::symmetric(14, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(32.0);
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 3.5, p.erro);
                ui.label(egui::RichText::new(texto).color(p.texto).size(13.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    tentar = tema::botao_secundario(ui, "Tentar de novo").clicked();
                });
            });
        });
    tentar
}

/// Estado vazio de uma página: logo, título, explicação e, se houver, um botão.
pub fn vazio(ui: &mut egui::Ui, titulo: &str, texto: &str, botao: Option<&str>) -> bool {
    let p = cores();
    let mut clicou = false;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.18).max(24.0));
        ui.allocate_ui(vec2(420.0, 0.0), |ui| {
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                tema::logo(ui.painter(), r.center(), 22.0);
                ui.add_space(16.0);
                ui.label(tema::texto_forte(titulo, 17.0).color(p.texto));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(texto).color(p.suave).size(13.5));
                if let Some(b) = botao {
                    ui.add_space(16.0);
                    clicou = tema::botao_principal(ui, b, true).clicked();
                }
            });
        });
    });
    clicou
}

/// Fileira de números grandes da Daily, da Sprint, da capa e do slide final:
/// os mesmos números, com os mesmos nomes, nas três telas.
pub fn numeros(ui: &mut egui::Ui, n: &api::NumerosCapa, tamanho: f32, com_novas: bool) {
    let p = cores();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = if tamanho >= 48.0 { 56.0 } else { 40.0 };
        let mut ultima = None;
        for (valor, rotulo, ponto) in numeros_visiveis(n) {
            ultima = Some(tema::metrica(ui, &valor, rotulo, ponto, tamanho));
        }
        let novas = texto_novas(n.novas);
        if com_novas && !novas.is_empty() {
            // Na linha dos rótulos, embaixo.
            let altura = ultima.map_or(0.0, |r| r.rect.height());
            ui.vertical(|ui| {
                ui.add_space((altura - 18.0).max(0.0));
                ui.label(egui::RichText::new(novas).color(p.suave).size(13.5));
            });
        }
    });
}

/// Os tópicos de "o que foi feito" de um slide, juntando as partes.
fn topicos(s: &api::Slide) -> Vec<&str> {
    s.feito.iter().flat_map(|f| f.itens.iter().map(String::as_str)).collect()
}

const ALTURA_TOPICO: f32 = 20.0;
const LADO_MINIATURA: f32 = 64.0;

/// Altura do cartão de tarefa da Daily e da Sprint numa largura.
pub fn altura_cartao(ui: &egui::Ui, s: &api::Slide, largura: f32) -> f32 {
    let p = cores();
    let interno = largura - 32.0;
    let titulo = texto_em_linhas(ui.painter(), &s.titulo, forte(15.0), p.texto, interno, 2).size().y;
    let total = topicos(s).len();
    let mut altura = 16.0 + titulo + 8.0 + 22.0;
    if total > 0 {
        altura += 8.0 + total.min(3) as f32 * ALTURA_TOPICO + if total > 3 { 18.0 } else { 0.0 };
    }
    let nota = if s.nota.is_empty() { None } else { Some(&s.nota) };
    if let Some(nota) = nota {
        let g = texto_em_linhas(ui.painter(), nota, FontId::proportional(13.5), p.texto, interno - 12.0, 2);
        altura += 10.0 + g.size().y;
    }
    if !s.anexos.is_empty() {
        altura += 12.0 + LADO_MINIATURA;
    }
    altura + 16.0
}

/// Cartão de uma tarefa (Daily e Sprint): título, etiquetas, até 3 tópicos,
/// a nota e as miniaturas. O clique abre a apresentação no slide dela.
#[allow(clippy::too_many_arguments)]
pub fn cartao(ui: &mut egui::Ui, rect: Rect, s: &api::Slide, com_projeto: bool, cache: &mut CacheImagens, rolar_ate: &mut Option<i64>) -> egui::Response {
    let p = cores();
    let resposta = ui.interact(rect, Id::new(("cartao-registro", s.tarefa_id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    if *rolar_ate == Some(s.tarefa_id) {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
        *rolar_ate = None;
    }
    let borda = if resposta.hovered() { p.destaque.gamma_multiply(0.55) } else { p.borda };
    let pintor = ui.painter().clone();
    pintor.rect(rect, CornerRadius::same(tema::RAIO_CARTAO), p.superficie_alta, Stroke::new(1.0, borda), StrokeKind::Inside);
    let interno = rect.shrink(16.0);
    let mut y = interno.top();
    let titulo = texto_em_linhas(&pintor, &s.titulo, forte(15.0), p.texto, interno.width(), 2);
    let altura_titulo = titulo.size().y;
    pintor.galley(pos2(interno.left(), y), titulo, p.texto);
    y += altura_titulo + 8.0;

    // Etiquetas: projeto, estado e as marcas Ontem / Hoje.
    let mut x = interno.left();
    if com_projeto && !s.projeto.is_empty() {
        x = tema::etiqueta(&pintor, pos2(x, y + 1.5), &s.projeto, tema::fonte_etiqueta(), p.suave).right() + 8.0;
    }
    x = pilula_do_slide(s, false).pintar(&pintor, pos2(x, y)).right() + 8.0;
    for parte in &s.partes {
        if parte == "No período" {
            continue;
        }
        let cor = if parte == "Hoje" { p.destaque } else { p.suave };
        x = tema::etiqueta(&pintor, pos2(x, y + 1.5), parte, tema::fonte_etiqueta(), cor).right() + 6.0;
    }
    y += 22.0;

    let todos = topicos(s);
    if !todos.is_empty() {
        y += 8.0;
        for t in todos.iter().take(3) {
            let meio = y + ALTURA_TOPICO / 2.0;
            pintor.circle_filled(pos2(interno.left() + 3.0, meio), 2.0, p.suave);
            texto_cortado(&pintor, pos2(interno.left() + 12.0, meio), t, FontId::proportional(13.5), p.texto, interno.width() - 12.0);
            y += ALTURA_TOPICO;
        }
        if todos.len() > 3 {
            let n = todos.len() - 3;
            let texto = if n == 1 { "+1 item".to_string() } else { format!("+{n} itens") };
            pintor.text(pos2(interno.left() + 12.0, y + 8.0), egui::Align2::LEFT_CENTER, texto, FontId::proportional(12.5), p.suave);
            y += 18.0;
        }
    }
    if !s.nota.is_empty() {
        y += 10.0;
        let g = texto_em_linhas(&pintor, &s.nota, FontId::proportional(13.5).clone(), p.texto, interno.width() - 12.0, 2);
        let altura = g.size().y;
        pintor.rect_filled(Rect::from_min_size(pos2(interno.left(), y), vec2(2.0, altura)), CornerRadius::same(1), p.destaque);
        let formato = egui::TextFormat { font_id: FontId::proportional(13.5), color: p.texto, italics: true, ..Default::default() };
        pintor.galley(pos2(interno.left() + 12.0, y), tema::cortar(&pintor, &s.nota, formato, interno.width() - 12.0, 2, false), p.texto);
        y += altura;
    }
    if !s.anexos.is_empty() {
        y += 12.0;
        let mut x = interno.left();
        for (n, a) in s.anexos.iter().enumerate() {
            let caixa = Rect::from_min_size(pos2(x, y), vec2(LADO_MINIATURA, LADO_MINIATURA));
            if n == 3 {
                caixa_mais(&pintor, caixa, s.anexos.len() - 3);
                break;
            }
            if a.video() {
                pintor.rect(caixa, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie, Stroke::new(1.0, p.borda), StrokeKind::Inside);
                pintor.circle_filled(caixa.center(), 12.0, p.destaque);
                tema::play(&pintor, caixa.center(), 5.0, tema::sobre_destaque());
            } else {
                // A miniatura só desenha: o clique é do cartão inteiro.
                let ctx = ui.ctx().clone();
                let raio = CornerRadius::same(tema::RAIO_CONTROLE);
                match cache.pedir(&ctx, a.id) {
                    Miniatura::Pronta(t) => {
                        let tamanho = t.size_vec2();
                        let escala = (caixa.width() / tamanho.x).max(caixa.height() / tamanho.y);
                        let visivel = vec2(caixa.width() / (tamanho.x * escala), caixa.height() / (tamanho.y * escala));
                        egui::Image::new(t).uv(Rect::from_center_size(pos2(0.5, 0.5), visivel)).corner_radius(raio).paint_at(ui, caixa);
                    }
                    _ => {
                        pintor.rect_filled(caixa, raio, p.realce);
                    }
                }
                pintor.rect_stroke(caixa, raio, Stroke::new(1.0, p.borda), StrokeKind::Inside);
            }
            x += LADO_MINIATURA + 8.0;
        }
    }
    // Sem dica: o cursor de mão e a borda realçada já dizem que abre o slide.
    resposta
}

/// A pílula de estado de um slide: a palavra e a marca de `estado_do_slide`.
/// "Removida" é a pílula neutra (fundo de cartão e borda), como a do projeto.
pub fn pilula_do_slide(s: &api::Slide, grande: bool) -> Pilula<'static> {
    let (cor, rotulo, marca) = estado_do_slide(s);
    let pilula = if s.removida { Pilula::neutra(rotulo) } else { Pilula { texto: rotulo, cor, cheio: marca == Marca::Cheia, ponto: true, grande: false } };
    if grande { pilula.grande() } else { pilula }
}

/// Largura reservada para a barra de rolagem dentro das áreas roláveis: o
/// conteúdo termina antes dela, sem nada por baixo.
pub const MARGEM_ROLAGEM: f32 = 14.0;

/// Sombra na borda de cima de uma área rolada (o conteúdo passa por baixo
/// do cabeçalho da página): só quando já rolou.
pub fn sombra_rolagem(pintor: &egui::Painter, area: Rect, deslocamento: f32) {
    if deslocamento <= 0.5 {
        return;
    }
    for i in 0..4 {
        let faixa = Rect::from_min_size(area.left_top() + vec2(0.0, i as f32 * 2.0), vec2(area.width(), 2.0));
        pintor.rect_filled(faixa, 0, Color32::from_black_alpha(if tema::claro() { 10 } else { 26 } / (i + 1)));
    }
}

/// Grade de cartões (2 colunas a partir de 1100 px, 1 abaixo), com a altura
/// do maior em cada linha. Devolve a tarefa clicada.
pub fn grade(ui: &mut egui::Ui, slides: &[&api::Slide], com_projeto: bool, cache: &mut CacheImagens, rolar_ate: &mut Option<i64>) -> Option<i64> {
    let largura = ui.available_width();
    let colunas = if largura >= 1100.0 { 2 } else { 1 };
    let largura_cartao = (largura - 12.0 * (colunas as f32 - 1.0)) / colunas as f32;
    let mut clicada = None;
    for linha in slides.chunks(colunas) {
        let altura = linha.iter().map(|s| altura_cartao(ui, s, largura_cartao)).fold(0.0, f32::max);
        let (faixa, _) = ui.allocate_exact_size(vec2(largura, altura), Sense::hover());
        for (i, s) in linha.iter().enumerate() {
            let rect = Rect::from_min_size(faixa.min + vec2(i as f32 * (largura_cartao + 12.0), 0.0), vec2(largura_cartao, altura));
            if (ui.is_rect_visible(rect) || *rolar_ate == Some(s.tarefa_id)) && cartao(ui, rect, s, com_projeto, cache, rolar_ate).clicked() {
                clicada = Some(s.tarefa_id);
            }
        }
        ui.add_space(12.0 - ui.spacing().item_spacing.y);
    }
    clicada
}

/// Esqueleto do estado "carregando": os números e 4 cartões, parados.
pub fn esqueleto_pagina(ui: &mut egui::Ui) {
    let largura = ui.available_width();
    let (area, _) = ui.allocate_exact_size(vec2(largura, 72.0 + 28.0 + 2.0 * 152.0), Sense::hover());
    let pintor = ui.painter();
    for i in 0..5 {
        let x = area.left() + i as f32 * 112.0;
        tema::esqueleto(pintor, Rect::from_min_size(pos2(x, area.top()), vec2(72.0, 32.0)), tema::RAIO_ETIQUETA);
        tema::esqueleto(pintor, Rect::from_min_size(pos2(x, area.top() + 40.0), vec2(56.0, 12.0)), tema::RAIO_ETIQUETA);
    }
    let colunas = if largura >= 1100.0 { 2 } else { 1 };
    let lc = (largura - 12.0 * (colunas as f32 - 1.0)) / colunas as f32;
    for i in 0..4 {
        let (l, c) = (i / colunas, i % colunas);
        let min = pos2(area.left() + c as f32 * (lc + 12.0), area.top() + 100.0 + l as f32 * 152.0);
        if min.y + 140.0 <= area.bottom() + 1.0 {
            tema::esqueleto(pintor, Rect::from_min_size(min, vec2(lc, 140.0)), tema::RAIO_CARTAO);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_da_tela_viram_datas_do_nucleo() {
        assert_eq!(data_da_tela("25/09/2026").as_deref(), Some("2026-09-25"));
        assert_eq!(data_da_tela(" 01/10/2026 ").as_deref(), Some("2026-10-01"));
        assert!(data_da_tela("2026-09-25").is_none());
        assert!(data_da_tela("1/9/2026").is_none());
        assert!(data_da_tela("aa/bb/cccc").is_none());
    }

    #[test]
    fn miniatura_reduz_a_imagem() {
        let mut png = Vec::new();
        let mut c = png::Encoder::new(&mut png, 400, 300);
        c.set_color(png::ColorType::Rgb);
        c.set_depth(png::BitDepth::Eight);
        let mut e = c.write_header().unwrap();
        e.write_image_data(&vec![200u8; 400 * 300 * 3]).unwrap();
        e.finish().unwrap();
        let img = decodificar(&png, Some(128)).unwrap();
        assert!(img.size[1] <= 128 && img.size[0] > 128);
        assert_eq!(img.pixels[0], Color32::from_rgb(200, 200, 200));
        let inteira = decodificar(&png, None).unwrap();
        assert_eq!(inteira.size, [400, 300]);
    }

    #[test]
    fn duracao_e_tamanho_em_portugues() {
        assert_eq!(duracao(30), "menos de 1 min");
        assert_eq!(duracao(720), "12 min");
        assert_eq!(duracao(3900), "1h05");
        assert_eq!(tamanho(12_900_000), "12,3 MB");
        assert_eq!(tamanho(700 << 20), "700,0 MB");
        assert_eq!(tamanho(2048), "2 KB");
    }
}
