//! A conversa com a tela: uma linha de JSON por mensagem, pela entrada e pela
//! saída padrão deste processo. Nada passa por rede ou arquivo.

use serde::{Deserialize, Serialize};

/// O que a tela pede. Uma linha maior que isto é recusada.
pub const LIMITE_LINHA: usize = 1024;

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Comando {
    /// Abrir o microfone e começar a guardar o áudio (na memória).
    Ouvir,
    /// Fechar o microfone e transcrever o que ouviu.
    Parar,
    /// Fechar o microfone e jogar fora o que ouviu.
    Cancelar,
    /// Sair agora.
    Sair,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "evento", rename_all = "snake_case")]
pub enum Evento {
    /// O processo está pronto para receber pedidos (o modelo carrega por trás).
    Iniciado,
    /// O microfone abriu.
    Ouvindo,
    /// O microfone fechou e a transcrição começou.
    Transcrevendo { segundos: f32 },
    /// O texto transcrito, já limpo (pode vir vazio).
    Texto { texto: String },
    /// Algo deu errado; a mensagem é para quem usa.
    Erro { mensagem: String },
    /// O processo vai sair (`parado`: muito tempo sem uso).
    Saindo { motivo: &'static str },
}

/// `{"comando": "..."}` e nada mais (o serde não recusa campos a mais num
/// enum com a etiqueta dentro, então a etiqueta vem num envelope).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    comando: Comando,
}

pub fn ler(linha: &str) -> Result<Comando, String> {
    if linha.len() > LIMITE_LINHA {
        return Err("Pedido grande demais para o ditado.".into());
    }
    serde_json::from_str::<Envelope>(linha.trim()).map(|e| e.comando).map_err(|_| "Pedido desconhecido para o ditado.".into())
}

pub fn linha(evento: &Evento) -> String {
    // Um Evento sempre vira JSON: só tem textos e números.
    serde_json::to_string(evento).unwrap_or_default()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_os_comandos_conhecidos() {
        assert_eq!(ler(r#"{"comando":"ouvir"}"#), Ok(Comando::Ouvir));
        assert_eq!(ler(" {\"comando\":\"parar\"}\n"), Ok(Comando::Parar));
        assert_eq!(ler(r#"{"comando":"cancelar"}"#), Ok(Comando::Cancelar));
        assert_eq!(ler(r#"{"comando":"sair"}"#), Ok(Comando::Sair));
    }

    #[test]
    fn recusa_o_resto() {
        for l in ["", "ouvir", r#"{"comando":"gravar"}"#, r#"{"comando":"ouvir","arquivo":"/tmp/x"}"#, "[1]"] {
            assert!(ler(l).is_err(), "{l}");
        }
        let grande = format!(r#"{{"comando":"ouvir"{}}}"#, " ".repeat(LIMITE_LINHA));
        assert!(ler(&grande).is_err());
    }

    #[test]
    fn escreve_uma_linha_de_json() {
        assert_eq!(linha(&Evento::Texto { texto: "olá \"mundo\"".into() }), r#"{"evento":"texto","texto":"olá \"mundo\""}"#);
        assert_eq!(linha(&Evento::Saindo { motivo: "parado" }), r#"{"evento":"saindo","motivo":"parado"}"#);
        assert_eq!(linha(&Evento::Iniciado), r#"{"evento":"iniciado"}"#);
    }
}
