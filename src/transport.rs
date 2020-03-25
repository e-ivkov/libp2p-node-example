use futures::io::{AsyncRead, AsyncWrite};
use futures::Future;
use libp2p::{core, identity, mplex, secio, yamux, PeerId, Transport};
use std::time::Duration;
use std::{error, io};

pub fn upgrade_dev_transport<T, O, E, L, D, U>(
    transport: T,
    keypair: identity::Keypair,
) -> io::Result<
    impl Transport<
            Output = (
                PeerId,
                impl core::muxing::StreamMuxer<
                        OutboundSubstream = impl Send,
                        Substream = impl Send,
                        Error = impl Into<io::Error>,
                    > + Send
                    + Sync,
            ),
            Error = impl error::Error + Send,
            Listener = impl Send,
            Dial = impl Send,
            ListenerUpgrade = impl Send,
        > + Clone,
>
where
    T: Transport<Output = O, Error = E, Listener = L, Dial = D, ListenerUpgrade = U> + Clone + Send,
    O: AsyncWrite + AsyncRead + Unpin + Send + 'static,
    E: Send + error::Error + 'static,
    L: Send
        + futures::stream::Stream<
            Item = Result<core::transport::ListenerEvent<T::ListenerUpgrade, E>, E>,
        >,
    D: Send + Future<Output = Result<O, E>>,
    U: Send + Future<Output = Result<O, E>>,
{
    Ok(transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(secio::SecioConfig::new(keypair))
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux::Config::default(),
            mplex::MplexConfig::new(),
        ))
        .map(|(peer, muxer), _| (peer, core::muxing::StreamMuxerBox::new(muxer)))
        .timeout(Duration::from_secs(20)))
}
