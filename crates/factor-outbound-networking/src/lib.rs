use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use futures_util::{future::{BoxFuture, Shared}, FutureExt};
use spin_factor_variables::VariablesFactor;
use spin_factor_wasi::WasiFactor;
use spin_factors::{Factor, FactorInstancePreparer, Result, SpinFactors};
use spin_outbound_networking::{AllowedHostsConfig, HostConfig, PortConfig, ALLOWED_HOSTS_KEY};

pub struct OutboundNetworkingFactor;

impl Factor for OutboundNetworkingFactor {
    type AppConfig = AppConfig;
    type InstancePreparer = InstancePreparer;
    type InstanceState = ();

    fn configure_app<Factors: SpinFactors>(
        &self,
        app: &spin_factors::App,
        _ctx: spin_factors::ConfigureAppContext<Factors>,
    ) -> Result<Self::AppConfig> {
        // Extract allowed_outbound_hosts for all components
        let component_allowed_hosts = app
            .components()
            .map(|component| {
                Ok((
                    component.id().to_string(),
                    component
                        .get_metadata(ALLOWED_HOSTS_KEY)?
                        .unwrap_or_default(),
                ))
            })
            .collect::<Result<_>>()?;

        Ok(AppConfig {
            component_allowed_hosts,
        })
    }
}

#[derive(Default)]
pub struct AppConfig {
    component_allowed_hosts: HashMap<String, Vec<String>>,
}

type AllowedHostsFuture = Shared<BoxFuture<'static, Arc<anyhow::Result<AllowedHostsConfig>>>>;

pub struct InstancePreparer {
    allowed_hosts_future: AllowedHostsFuture,
}

impl FactorInstancePreparer<OutboundNetworkingFactor> for InstancePreparer {
    fn new<Factors: SpinFactors>(
        _factor: &OutboundNetworkingFactor,
        app_component: &spin_factors::AppComponent,
        mut ctx: spin_factors::PrepareContext<Factors>,
    ) -> Result<Self> {
        let hosts = 
            ctx.app_config::<OutboundNetworkingFactor>()
                .unwrap()
                .component_allowed_hosts
                .get(app_component.id())
                .context("missing component allowed hosts")?;
        let resolver = ctx.instance_preparer_mut::<VariablesFactor>()?.resolver().clone();
        let allowed_hosts_future = async move {
            let prepared = resolver.prepare().await?;
            Ok(AllowedHostsConfig::parse(&hosts, &prepared))
        }.boxed().shared();
        let prepared_resolver = resolver.prepare().await?;
        let allowed_hosts = AllowedHostsConfig::parse(
                .context("missing component allowed hosts")?,
            &prepared_resolver,
        )?;

        // Update Wasi socket allowed ports
        let wasi_preparer = ctx.instance_preparer_mut::<WasiFactor>()?;
        match &allowed_hosts {
            AllowedHostsConfig::All => wasi_preparer.inherit_network(),
            AllowedHostsConfig::SpecificHosts(configs) => {
                for config in configs {
                    if config.scheme().allows_any() {
                        match (config.host(), config.port()) {
                            (HostConfig::Cidr(ip_net), PortConfig::Any) => {
                                wasi_preparer.socket_allow_ports(*ip_net, 0, None)
                            }
                            _ => todo!(), // TODO: complete and validate against existing Network TriggerHooks
                        }
                    }
                }
            }
        }

        Ok(Self {
            allowed_hosts: Arc::new(allowed_hosts),
        })
    }

    fn prepare(self) -> Result<<OutboundNetworkingFactor as Factor>::InstanceState> {
        Ok(())
    }
}

impl InstancePreparer {
    pub async fn resolve_allowed_hosts(&self) -> Arc<anyhow::Result<AllowedHostsConfig>> {
        self.allowed_hosts_future.clone().await
    }
}
