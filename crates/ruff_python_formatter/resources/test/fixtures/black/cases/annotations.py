# regression test for #1765
class Foo:
    def foo(self):
        if True:
            content_ids: Mapping[
                str, Optional[ContentId]
            ] = self.publisher_content_store.store_config_contents(files)
