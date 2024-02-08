//! Customizable node builder.

use crate::{
    components::{FullNodeComponents, FullNodeComponentsAdapter, NodeComponentsBuilder},
    hooks::NodeHooks,
    node::{FullNode, FullNodeTypes, FullNodeTypesAdapter, NodeTypes},
    rpc::{RethRpcServerHandles, RpcContext, RpcHooks},
    NodeHandle,
};
use reth_blockchain_tree::ShareableBlockchainTree;
use reth_db::{
    database::Database,
    database_metrics::{DatabaseMetadata, DatabaseMetrics},
};
use reth_node_core::{
    cli::config::RethTransactionPoolConfig,
    dirs::{ChainPath, DataDirPath},
    node_config::NodeConfig,
    primitives::{kzg::KzgSettings, Head},
};
use reth_primitives::{
    constants::eip4844::{LoadKzgSettingsError, MAINNET_KZG_TRUSTED_SETUP},
    ChainSpec,
};
use reth_provider::{providers::BlockchainProvider, ChainSpecProvider};
use reth_revm::EvmProcessorFactory;
use reth_tasks::TaskExecutor;
use reth_transaction_pool::PoolConfig;
use std::{marker::PhantomData, sync::Arc};

/// The builtin provider type of the reth node.
// Note: we need to hardcode this because custom components might depend on it in associated types.
// TODO: this will eventually depend on node primitive types and evm
type RethFullProviderType<DB, Evm> =
    BlockchainProvider<DB, ShareableBlockchainTree<DB, EvmProcessorFactory<Evm>>>;

/// Declaratively construct a node.
///
/// [`NodeBuilder`] provides a [builder-like interface][builder] for composing
/// components of a node.
///
/// Configuring a node starts out with a [`NodeConfig`] and then proceeds to configure the core
/// static types of the node: [NodeTypes], these include the node's primitive types and the node's
/// engine types.
///
/// Next all stateful components of the node are configured, these include the
/// [EvmConfig](reth_node_api::evm::EvmConfig), the database [Database] and finally all the
/// components of the node that are downstream of those types, these include:
///
///  - The transaction pool: [PoolBuilder](crate::components::PoolBuilder)
///  - The network: [NetworkBuilder](crate::components::NetworkBuilder)
///  - The payload builder: [PayloadBuilder](crate::components::PayloadServiceBuilder)
///
/// Finally, the node is ready to launch [NodeBuilder::launch]
///
/// [builder]: https://doc.rust-lang.org/1.0.0/style/ownership/builders.html
pub struct NodeBuilder<DB, State> {
    /// All settings for how the node should be configured.
    config: NodeConfig,
    /// State of the node builder process.
    state: State,
    /// The configured database for the node.
    database: DB,
}

impl<DB, State> NodeBuilder<DB, State> {
    /// Returns a reference to the node builder's config.
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }
}

impl NodeBuilder<(), InitState> {
    /// Create a new [`NodeBuilder`].
    pub fn new(config: NodeConfig) -> Self {
        Self { config, database: (), state: InitState::default() }
    }
}

impl<DB> NodeBuilder<DB, InitState> {
    /// Configures the additional external context, e.g. additional context captured via CLI args.
    pub fn with_database<D>(self, database: D) -> NodeBuilder<D, InitState> {
        NodeBuilder { config: self.config, state: self.state, database }
    }
}

impl<DB> NodeBuilder<DB, InitState>
where
    DB: Database + Clone + 'static,
{
    /// Configures the types of the node.
    pub fn with_types<T>(self, types: T) -> NodeBuilder<DB, TypesState<T, DB>>
    where
        T: NodeTypes,
    {
        NodeBuilder {
            config: self.config,
            state: TypesState { types, adapter: FullNodeTypesAdapter::default() },
            database: self.database,
        }
    }
}

impl<DB, Types> NodeBuilder<DB, TypesState<Types, DB>>
where
    Types: NodeTypes,
    DB: Database + Clone + Unpin + 'static,
{
    /// Configures the node's components.
    pub fn with_components<Components>(
        self,
        components_builder: Components,
    ) -> NodeBuilder<
        DB,
        ComponentsState<
            Types,
            Components,
            FullNodeComponentsAdapter<
                FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                Components::Pool,
            >,
        >,
    >
    where
        Components: NodeComponentsBuilder<
            FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
        >,
    {
        NodeBuilder {
            config: self.config,
            database: self.database,
            state: ComponentsState {
                _maker: Default::default(),
                components_builder,
                hooks: NodeHooks::new(),
                rpc: RpcHooks::new(),
            },
        }
    }
}

impl<DB, Types, Components>
    NodeBuilder<
        DB,
        ComponentsState<
            Types,
            Components,
            FullNodeComponentsAdapter<
                FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                Components::Pool,
            >,
        >,
    >
where
    DB: Database + DatabaseMetrics + DatabaseMetadata + Clone + Unpin + 'static,
    Types: NodeTypes,
    Components: NodeComponentsBuilder<
        FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
    >,
{
    /// Apply a function to the components builder.
    pub fn map_components(self, f: impl FnOnce(Components) -> Components) -> Self {
        Self {
            config: self.config,
            database: self.database,
            state: ComponentsState {
                _maker: Default::default(),
                components_builder: f(self.state.components_builder),
                hooks: self.state.hooks,
                rpc: self.state.rpc,
            },
        }
    }

    /// Resets the setup process to the components stage.
    ///
    /// CAUTION: All previously configured hooks will be lost.
    pub fn fuse_components<C>(
        self,
        components_builder: C,
    ) -> NodeBuilder<
        DB,
        ComponentsState<
            Types,
            C,
            FullNodeComponentsAdapter<
                FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                C::Pool,
            >,
        >,
    >
    where
        C: NodeComponentsBuilder<
            FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
        >,
    {
        NodeBuilder {
            config: self.config,
            database: self.database,
            state: ComponentsState {
                _maker: Default::default(),
                components_builder,
                hooks: NodeHooks::new(),
                rpc: RpcHooks::new(),
            },
        }
    }

    /// Sets the hook that is run once the node's components are initialized.
    pub fn on_component_initialized<F>(mut self, hook: F) -> Self
    where
        F: FnOnce(
                FullNodeComponentsAdapter<
                    FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                    Components::Pool,
                >,
            ) -> eyre::Result<()>
            + 'static,
    {
        self.state.hooks.set_on_component_initialized(hook);
        self
    }

    /// Sets the hook that is run once the node has started.
    pub fn on_node_started<F>(mut self, hook: F) -> Self
    where
        F: FnOnce(
                FullNode<
                    FullNodeComponentsAdapter<
                        FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                        Components::Pool,
                    >,
                >,
            ) -> eyre::Result<()>
            + 'static,
    {
        self.state.hooks.set_on_node_started(hook);
        self
    }

    /// Sets the hook that is run once the rpc server is started.
    pub fn on_rpc_started<F>(mut self, hook: F) -> Self
    where
        F: FnOnce(
                RpcContext<
                    '_,
                    FullNodeComponentsAdapter<
                        FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                        Components::Pool,
                    >,
                >,
                RethRpcServerHandles,
            ) -> eyre::Result<()>
            + 'static,
    {
        self.state.rpc.set_on_rpc_started(hook);
        self
    }

    /// Sets the hook that is run to configure the rpc modules.
    pub fn extend_rpc_modules<F>(mut self, hook: F) -> Self
    where
        F: FnOnce(
                RpcContext<
                    '_,
                    FullNodeComponentsAdapter<
                        FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                        Components::Pool,
                    >,
                >,
            ) -> eyre::Result<()>
            + 'static,
    {
        self.state.rpc.set_extend_rpc_modules(hook);
        self
    }

    /// Launches the node and returns a handle to it.
    pub async fn launch(
        self,
        _executor: TaskExecutor,
    ) -> eyre::Result<
        NodeHandle<
            FullNodeComponentsAdapter<
                FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
                Components::Pool,
            >,
        >,
    > {
        // 0. blockchain provider setup
        // 1. create the `BuilderContext`
        // 2. build the components
        // 3. build/customize rpc
        // 4. apply hooks

        todo!()
    }

    /// Check that the builder can be launched
    ///
    /// This is useful when writing tests to ensure that the builder is configured correctly.
    pub fn check_launch(self) -> Self {
        self
    }
}

/// Captures the necessary context for building the components of the node.
#[derive(Debug)]
pub struct BuilderContext<Node: FullNodeTypes> {
    /// The current head of the blockchain at launch.
    head: Head,
    /// The configured provider to interact with the blockchain.
    provider: Node::Provider,
    /// The executor of the node.
    executor: TaskExecutor,
    /// The data dir of the node.
    data_dir: ChainPath<DataDirPath>,
    /// The config of the node
    config: NodeConfig,
}

impl<Node: FullNodeTypes> BuilderContext<Node> {
    pub fn provider(&self) -> &Node::Provider {
        &self.provider
    }

    /// Returns the current head of the blockchain at launch.
    pub fn head(&self) -> Head {
        self.head
    }

    /// Returns the config of the node.
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    /// Returns the data dir of the node.
    ///
    /// This gives access to all relevant files and directories of the node's datadir.
    pub fn data_dir(&self) -> &ChainPath<DataDirPath> {
        &self.data_dir
    }

    /// Returns the executor of the node.
    ///
    /// This can be used to execute async tasks or functions during the setup.
    pub fn executor(&self) -> &TaskExecutor {
        &self.executor
    }

    /// Returns the chain spec of the node.
    pub fn chain_spec(&self) -> Arc<ChainSpec> {
        self.provider().chain_spec()
    }

    /// Returns the transaction pool config of the node.
    pub fn pool_config(&self) -> PoolConfig {
        self.config().txpool.pool_config()
    }

    /// Loads the trusted setup params from a given file path or falls back to
    /// `MAINNET_KZG_TRUSTED_SETUP`.
    pub fn kzg_settings(&self) -> eyre::Result<Arc<KzgSettings>> {
        if let Some(ref trusted_setup_file) = self.config().trusted_setup_file {
            let trusted_setup = KzgSettings::load_trusted_setup_file(trusted_setup_file)
                .map_err(LoadKzgSettingsError::KzgError)?;
            Ok(Arc::new(trusted_setup))
        } else {
            Ok(Arc::clone(&MAINNET_KZG_TRUSTED_SETUP))
        }
    }
}

/// The initial state of the node builder process.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct InitState;

/// The state after all types of the node have been configured.
#[derive(Debug)]
pub struct TypesState<Types, DB>
where
    DB: Database + Clone + 'static,
    Types: NodeTypes,
{
    types: Types,
    adapter: FullNodeTypesAdapter<Types, DB, RethFullProviderType<DB, Types::Evm>>,
}

/// The state of the node builder process after the node's components have been configured.
///
/// With this state all types and components of the node are known and the node can be launched.
///
/// Additionally, this state captures additional hooks that are called at specific points in the
/// node's launch lifecycle.
#[derive(Debug)]
pub struct ComponentsState<Types, Components, FullNode: FullNodeComponents> {
    _maker: PhantomData<Types>,
    components_builder: Components,
    /// Additional NodeHooks that are called at specific points in the node's launch lifecycle.
    hooks: NodeHooks<FullNode>,
    rpc: RpcHooks<FullNode>,
}
