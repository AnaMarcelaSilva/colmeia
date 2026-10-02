//! Caixa de mensagem do painel da tarefa. Enter quebra a linha, Ctrl+Enter
//! envia, e Ctrl+V com uma imagem na área de transferência anexa a imagem:
//! ela é gravada como PNG numa pasta só do usuário e o caminho vai junto na
//! mensagem, que é como o Claude Code e o Codex recebem imagens.

use std::path::{Path, PathBuf};

use eframe::egui::{self, ColorImage, CornerRadius, Event, Key, Modifiers, TextureHandle, TextureOptions, vec2};

use crate::tema::{self, cores};

/// Maior imagem aceita (em pixels), para uma colagem acidental não travar a tela.
const MAIOR_IMAGEM: usize = 40_000_000;
const LADO_MINIATURA: f32 = 52.0;

struct Anexo {
    caminho: PathBuf,
    miniatura: TextureHandle,
}

#[derive(Default)]
pub struct Compositor {
    rascunho: String,
    pub para_todos: bool,
    pub focar: bool,
    anexos: Vec<Anexo>,
    aviso: Option<String>,
    /// Quando houve a última colagem de texto, para não tratar o mesmo Ctrl+V como imagem.
    colou_texto_em: f64,
}

/// O que enviar e para quem.
pub struct Envio {
    pub destinos: Vec<usize>,
    pub texto: String,
}

impl Compositor {
    /// Altura que a caixa precisa: cresce com o texto até 5 linhas, mais a
    /// fileira de imagens e a linha de dica.
    pub fn altura(&self) -> f32 {
        let linhas = self.rascunho.split('\n').count().clamp(1, 5) as f32;
        let imagens = if self.anexos.is_empty() { 0.0 } else { LADO_MINIATURA + 10.0 };
        54.0 + 19.0 * (linhas - 1.0) + imagens + 24.0
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, area: egui::Rect, agentes: &[usize], foco: usize, papeis: &[&str]) -> Option<Envio> {
        let p = cores();
        let id = egui::Id::new("compositor");
        if std::mem::take(&mut self.focar) {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let com_foco = ui.memory(|m| m.has_focus(id));
        // Ctrl+Enter envia; é retirado da fila antes de o campo ver, para não virar quebra de linha.
        let mut enviar = com_foco && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        // O egui só avisa um Ctrl+V quando há texto para colar. Sem texto (só uma
        // imagem), o que chega é a tecla V sendo solta com o Ctrl ainda apertado.
        let (agora, colou_texto, soltou_ctrl_v) = ui.input(|i| {
            (
                i.time,
                i.events.iter().any(|e| matches!(e, Event::Paste(_))),
                i.events.iter().any(|e| matches!(e, Event::Key { key: Key::V, pressed: false, modifiers, .. } if modifiers.command)),
            )
        });
        if colou_texto {
            self.colou_texto_em = agora;
        }
        if com_foco && soltou_ctrl_v && agora - self.colou_texto_em > 0.5 {
            self.colar_imagem(ui.ctx());
        }

        let caixa = egui::Rect::from_min_size(area.min, vec2(area.width(), area.height() - 24.0));
        let contorno = if com_foco { egui::Stroke::new(1.5, p.destaque) } else { egui::Stroke::new(1.0, p.borda) };
        ui.painter().rect(caixa, CornerRadius::same(18), p.superficie_alta, contorno, egui::StrokeKind::Inside);

        let mut interno = caixa.shrink2(vec2(10.0, 10.0));
        if !self.anexos.is_empty() {
            let fileira = egui::Rect::from_min_size(interno.min + vec2(4.0, 0.0), vec2(interno.width(), LADO_MINIATURA));
            self.miniaturas(ui, fileira);
            interno.min.y += LADO_MINIATURA + 10.0;
        }

        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(interno).layout(egui::Layout::left_to_right(egui::Align::Min)));
        let destino = if self.para_todos { format!("todos ({})", agentes.len()) } else { format!("agente-{foco} · {}", papeis[foco]) };
        let resposta = tema::chip(&mut filho, "Para", &destino, self.para_todos);
        egui::Popup::menu(&resposta).show(|ui| {
            ui.set_min_width(250.0);
            if tema::opcao_menu(ui, &format!("agente-{foco} ({}, em foco)", papeis[foco]), !self.para_todos) {
                self.para_todos = false;
                ui.close();
            }
            if tema::opcao_menu(ui, &format!("Todos os {} agentes da tarefa", agentes.len()), self.para_todos) {
                self.para_todos = true;
                ui.close();
            }
        });
        filho.add_space(6.0);
        let tem_conteudo = !self.rascunho.trim().is_empty() || !self.anexos.is_empty();
        filho.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if tema::botao_principal(ui, "Enviar", tem_conteudo).clicked() {
                enviar = true;
            }
            ui.add_space(8.0);
            let dica = if self.para_todos { "Mensagem para todos os agentes da tarefa".to_string() } else { format!("Mensagem para agente-{foco}") };
            let campo = egui::TextEdit::multiline(&mut self.rascunho)
                .id(id)
                .frame(egui::Frame::NONE)
                .desired_rows(1)
                .desired_width(ui.available_width())
                .font(egui::FontId::proportional(14.0))
                .margin(egui::Margin::symmetric(4, 7))
                .hint_text(dica);
            ui.add(campo);
        });

        let (texto_dica, cor_dica) = match &self.aviso {
            Some(aviso) => (aviso.as_str(), p.erro),
            None => ("Ctrl+Enter envia · Enter quebra a linha · Ctrl+V cola imagens · clique no terminal para digitar direto nele", p.suave),
        };
        ui.painter().text(
            egui::pos2(caixa.left() + 14.0, caixa.bottom() + 12.0),
            egui::Align2::LEFT_CENTER,
            texto_dica,
            egui::FontId::proportional(11.5),
            cor_dica,
        );

        if !(enviar && tem_conteudo) {
            return None;
        }
        let mut texto = self.rascunho.trim().to_string();
        if !self.anexos.is_empty() {
            let caminhos: Vec<String> = self.anexos.iter().map(|a| a.caminho.display().to_string()).collect();
            let rotulo = if caminhos.len() == 1 { "Imagem anexada:" } else { "Imagens anexadas:" };
            texto = format!("{texto}\n\n{rotulo}\n{}", caminhos.join("\n")).trim().to_string();
        }
        self.rascunho.clear();
        self.anexos.clear();
        self.aviso = None;
        ui.memory_mut(|m| m.request_focus(id));
        Some(Envio { destinos: if self.para_todos { agentes.to_vec() } else { vec![foco] }, texto })
    }

    fn miniaturas(&mut self, ui: &mut egui::Ui, fileira: egui::Rect) {
        let p = cores();
        let mut remover = None;
        for (i, anexo) in self.anexos.iter().enumerate() {
            let caixa = egui::Rect::from_min_size(fileira.min + vec2(i as f32 * (LADO_MINIATURA + 8.0), 0.0), vec2(LADO_MINIATURA, LADO_MINIATURA));
            let tamanho = anexo.miniatura.size_vec2();
            // Preenche o quadrado cortando o excesso, sem distorcer a imagem.
            let escala = (LADO_MINIATURA / tamanho.x).max(LADO_MINIATURA / tamanho.y);
            let visivel = vec2(LADO_MINIATURA / (tamanho.x * escala), LADO_MINIATURA / (tamanho.y * escala));
            let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), visivel);
            egui::Image::new(&anexo.miniatura).uv(uv).corner_radius(8).paint_at(ui, caixa);
            ui.painter().rect_stroke(caixa, 8, egui::Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
            let x = egui::Rect::from_center_size(caixa.right_top() + vec2(-6.0, 6.0), vec2(18.0, 18.0));
            let resposta = ui.interact(x, ui.id().with(("remover-anexo", i)), egui::Sense::click()).on_hover_text("Tirar imagem");
            ui.painter().circle_filled(x.center(), 9.0, if resposta.hovered() { p.erro } else { p.texto.gamma_multiply(0.75) });
            ui.painter().text(x.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(13.0), p.superficie_alta);
            if resposta.clicked() {
                remover = Some(i);
            }
        }
        if let Some(i) = remover {
            let anexo = self.anexos.remove(i);
            let _ = std::fs::remove_file(anexo.caminho);
        }
    }

    fn colar_imagem(&mut self, ctx: &egui::Context) {
        let Ok(mut area) = arboard::Clipboard::new() else { return };
        // Sem imagem na área de transferência é uma colagem de texto comum.
        let Ok(imagem) = area.get_image() else { return };
        if imagem.width * imagem.height > MAIOR_IMAGEM {
            self.aviso = Some("Imagem grande demais para anexar.".into());
            return;
        }
        match gravar_png(&imagem) {
            Ok(caminho) => {
                let cor = ColorImage::from_rgba_unmultiplied([imagem.width, imagem.height], &imagem.bytes);
                let miniatura = ctx.load_texture(caminho.display().to_string(), cor, TextureOptions::LINEAR);
                self.anexos.push(Anexo { caminho, miniatura });
                self.aviso = None;
            }
            Err(e) => self.aviso = Some(format!("Não consegui guardar a imagem: {e}")),
        }
    }
}

/// Grava a imagem como PNG em <dados>/anexos, com permissão só do usuário.
fn gravar_png(imagem: &arboard::ImageData) -> Result<PathBuf, String> {
    let dir = crate::canal::diretorio_dados().join("anexos");
    criar_pasta_privada(&dir).map_err(|e| e.to_string())?;
    let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let caminho = dir.join(format!("imagem-{}-{:03}.png", agora.as_secs(), agora.subsec_millis()));
    let arquivo = criar_arquivo_privado(&caminho).map_err(|e| e.to_string())?;
    let mut codificador = png::Encoder::new(std::io::BufWriter::new(arquivo), imagem.width as u32, imagem.height as u32);
    codificador.set_color(png::ColorType::Rgba);
    codificador.set_depth(png::BitDepth::Eight);
    let mut escritor = codificador.write_header().map_err(|e| e.to_string())?;
    escritor.write_image_data(&imagem.bytes).map_err(|e| e.to_string())?;
    Ok(caminho)
}

#[cfg(unix)]
fn criar_pasta_privada(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(unix)]
fn criar_arquivo_privado(caminho: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(caminho)
}

#[cfg(not(unix))]
fn criar_pasta_privada(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(not(unix))]
fn criar_arquivo_privado(caminho: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(caminho)
}
