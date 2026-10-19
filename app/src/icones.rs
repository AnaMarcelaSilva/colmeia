//! As imagens dos workspaces, já como texturas. O núcleo diz a versão de
//! cada uma junto com os projetos; só uma versão nova busca a imagem, numa
//! thread, e a tela segue com a anterior (ou com a inicial) até ela chegar.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;

use crate::api;
use crate::dados::Projeto;

type Chegada = (i64, String, Result<egui::ColorImage, String>);

pub struct Icones {
    /// Por workspace: a versão pedida e a textura que já chegou.
    texturas: HashMap<i64, (String, Option<egui::TextureHandle>)>,
    envio: Sender<Chegada>,
    chegando: Receiver<Chegada>,
}

impl Default for Icones {
    fn default() -> Self {
        let (envio, chegando) = mpsc::channel();
        Icones { texturas: HashMap::new(), envio, chegando }
    }
}

impl Icones {
    /// Confere as versões dos workspaces dos projetos e pede as que mudaram.
    /// Sem mudança, só compara textos curtos.
    pub fn atualizar(&mut self, ctx: &egui::Context, projetos: &[Projeto]) {
        while let Ok((workspace, versao, resultado)) = self.chegando.try_recv() {
            if let Some((pedida, textura)) = self.texturas.get_mut(&workspace)
                && *pedida == versao
            {
                *textura = resultado.ok().map(|img| ctx.load_texture(format!("workspace-{workspace}-{versao}"), img, egui::TextureOptions::LINEAR));
            }
        }
        for p in projetos {
            let atual = self.texturas.get(&p.workspace_id).map(|(v, _)| v.as_str());
            if atual == Some(p.workspace_icone.as_str()) || (atual.is_none() && p.workspace_icone.is_empty()) {
                continue;
            }
            if p.workspace_icone.is_empty() {
                self.texturas.remove(&p.workspace_id);
                continue;
            }
            let (workspace, versao) = (p.workspace_id, p.workspace_icone.clone());
            let anterior = self.texturas.remove(&workspace).and_then(|(_, t)| t);
            self.texturas.insert(workspace, (versao.clone(), anterior));
            let envio = self.envio.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let resultado = api::ler_icone(workspace).and_then(|png| crate::registro::decodificar(&png, None));
                let _ = envio.send((workspace, versao, resultado));
                ctx.request_repaint();
            });
        }
    }

    pub fn do_workspace(&self, workspace: i64) -> Option<&egui::TextureHandle> {
        self.texturas.get(&workspace).and_then(|(_, t)| t.as_ref())
    }
}
