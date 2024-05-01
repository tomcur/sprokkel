use minijinja::{context, value::ViaDeserialize, Environment};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::types;
use crate::Ctx;

thread_local! {
    // There is one Renderer instance for rendering, which has a `minijinja::Environment` where a
    // `paginate` function is defined. Rendering happens on multiple threads, so we define thread
    // local `Paginator` instances here. Once rendering of a template is complete, if the template
    // called out to `paginate`, PAGINATOR becomes set. The renderer then knows it needs to
    // paginate. When rendering the template is finished, `PAGINATOR` is unset.
    static PAGINATOR: RefCell<Option<Paginator>> = RefCell::new(None);
    static PAGE_PERMALINK: RefCell<Option<Box<dyn Fn(u32) -> String>>> = RefCell::new(None);
}

struct Paginator {
    item_count: usize,
    per_page: u32,
    current_page: u32,
    last_page: u32,
    page_permalinks: Vec<String>,
}

impl Paginator {
    pub fn new(ctx: &Ctx, per_page: u32, item_count: usize) -> Self {
        PAGE_PERMALINK.with_borrow(|page_permalink| {
            let page_permalink = page_permalink.as_ref().unwrap();

            let last_page = (item_count / per_page as usize) as u32;
            Paginator {
                item_count,
                per_page,
                current_page: 0,
                last_page,
                page_permalinks: (0u32..=last_page)
                    .map(|page| ctx.path_to_absolute_url((*page_permalink)(page)).unwrap())
                    .collect(),
            }
        })
    }

    /// Increase the paginator by a page. Returns whether we are expecting another page.
    pub fn paginate(&mut self) -> bool {
        self.current_page += 1;
        self.current_page <= self.last_page
    }
}

fn pagination_reset() {
    PAGINATOR.with_borrow_mut(|pagination| pagination.take());
}

/// Template pagination function that can be added to a `minijinja::Environment`. This takes the
/// total number of items (either as a sequence or as a number) and the number of items to be
/// displayed per page.
///
/// The first call per template render sets up the paginator. Subsequent calls ignore the arguments
/// and return the same result.
fn gen_paginate(
    ctx: Ctx,
) -> impl Fn(&minijinja::Value, u32) -> Result<minijinja::Value, minijinja::Error> {
    move |items, per_page| {
        PAGINATOR.with_borrow_mut(|paginator| {
            if paginator.is_none() {
                let item_count = if items.is_number() {
                    usize::try_from(items.clone()).ok()
                } else if let Some(seq) = items.as_seq() {
                    Some(seq.item_count())
                } else {
                    None
                };
                let item_count = item_count.ok_or(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "`items` argument is neither a number nor a sequence",
                ))?;

                *paginator = Some(Paginator::new(&ctx, per_page, item_count));
            }
            let paginator = paginator.as_ref().unwrap();

            let page_start = paginator.current_page as usize * paginator.per_page as usize;
            let page_end = (page_start + paginator.per_page as usize).min(paginator.item_count);

            let is_first_page = paginator.current_page == 0;
            let is_last_page = paginator.current_page == paginator.last_page;

            Ok(minijinja::context! {
                item_count => paginator.item_count,
                page_count => paginator.last_page + 1,
                current_page => paginator.current_page,
                indices => (page_start..page_end).collect::<Vec<_>>(),
                is_first_page => is_first_page,
                is_last_page => is_last_page,
                previous => if is_first_page {
                    None
                } else {
                    Some(paginator.page_permalinks[(paginator.current_page-1) as usize].clone())
                },
                next => if is_last_page {
                    None
                } else {
                    Some(paginator.page_permalinks[(paginator.current_page+1) as usize].clone())
                },
                page_permalinks => paginator.page_permalinks,
            })
        })
    }
}

pub struct Renderer {
    ctx: Ctx,
    t: Environment<'static>,
}

#[derive(Clone, Copy, serde::Serialize)]
struct TemplateCtx<'ctx> {
    base_url: &'ctx str,
    entries: &'ctx HashMap<&'ctx str, &'ctx [types::Entry<'ctx>]>,
}

#[derive(Clone, Copy)]
pub struct RenderCtx<'ctx> {
    renderer: &'ctx Renderer,
    ctx: TemplateCtx<'ctx>,
}

/// Minijinja filter to add leading zeros to a numeric value.
fn leading_zeros(val: minijinja::Value, leading_zeros: u8) -> Result<String, minijinja::Error> {
    let num: i64 = val.try_into()?;
    let length = num.ilog10() + 1;
    let zeros = "0".repeat(leading_zeros.saturating_sub(length as u8) as usize);

    Ok(format!("{zeros}{num}"))
}

impl Renderer {
    pub fn build(ctx: &Ctx, template_path: impl AsRef<Path>) -> anyhow::Result<Renderer> {
        let mut t = Environment::new();
        t.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);

        t.add_function("paginate", gen_paginate(ctx.clone()));
        t.add_filter("leading_zeros", leading_zeros);

        {
            let ctx = ctx.clone();
            t.add_filter(
                "path_to_url",
                move |path: ViaDeserialize<PathBuf>| -> Result<String, minijinja::Error> {
                    let url = ctx.path_to_absolute_url(&*path).map_err(|_| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            format!("path is not valid unicode: {:?}", &path.0),
                        )
                    })?;

                    Ok(url)
                },
            );
        }

        t.set_loader(minijinja::path_loader(template_path));

        Ok(Renderer {
            ctx: ctx.clone(),
            t,
        })
    }

    pub fn render_context<'ctx>(
        &'ctx self,
        entries: &'ctx HashMap<&'ctx str, &'ctx [types::Entry<'ctx>]>,
    ) -> RenderCtx<'ctx> {
        RenderCtx {
            renderer: self,
            ctx: TemplateCtx {
                base_url: &self.ctx.base_url(),
                entries,
            },
        }
    }
}

impl RenderCtx<'_> {
    pub fn entry(
        &self,
        write: impl std::io::Write,
        entry: &types::Entry,
        referring_entries: &[&types::Entry<'_>],
    ) -> anyhow::Result<()> {
        let template = self
            .renderer
            .t
            .get_template(&format!("_{}.html", entry.meta.group))
            .or_else(|_| self.renderer.t.get_template("_entry.html"))?;

        let ctx = context! {
                referring_entries => referring_entries,
                entry => entry,
        };
        template.render_to_write(
            context! {
                ..ctx, ..minijinja::Value::from_serialize(&self.ctx)
            },
            write,
        )?;

        Ok(())
    }

    pub fn template(
        &self,
        template_path: impl AsRef<Path>,
        page_permalink: impl Fn(u32) -> String + 'static,
    ) -> anyhow::Result<impl Iterator<Item = anyhow::Result<(u32, String)>>> {
        PAGE_PERMALINK.set(Some(Box::new(page_permalink)));
        let template = self
            .renderer
            .t
            .get_template(template_path.as_ref().to_str().ok_or(anyhow::anyhow!(
                "template path is not Unicode: {:?}",
                template_path.as_ref()
            ))?)?;

        let content = template.render(context! {
            ..minijinja::Value::from_serialize(self.ctx),
        });

        let mut paginate = PAGINATOR.with_borrow_mut(|paginator| {
            paginator
                .as_mut()
                .map(|paginator| paginator.paginate())
                .unwrap_or(false)
        });

        // once https://github.com/rust-lang/rust/issues/117078 lands this can be rewritten to a
        // generator to ease the required memory a bit. i can't really be bothered making a custom
        // iterator at the moment
        let mut pages = vec![content
            .map(|content| (0, content))
            .map_err(anyhow::Error::from)];

        if pages[0].is_ok() && paginate {
            let mut page = 0;

            while paginate {
                page += 1;
                let content = template.render(context! {
                    ..minijinja::Value::from_serialize(self.ctx),
                });
                pages.push(
                    content
                        .map(|content| (page, content))
                        .map_err(anyhow::Error::from),
                );

                paginate = PAGINATOR.with_borrow_mut(|paginator| {
                    paginator
                        .as_mut()
                        .map(|paginator| paginator.paginate())
                        .unwrap_or(false)
                });
            }
        }

        pagination_reset();
        Ok(pages.into_iter())
    }
}
