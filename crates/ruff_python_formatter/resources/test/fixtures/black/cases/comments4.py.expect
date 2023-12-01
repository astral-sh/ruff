from com.my_lovely_company.my_lovely_team.my_lovely_project.my_lovely_component import (
    MyLovelyCompanyTeamProjectComponent,  # NOT DRY
)
from com.my_lovely_company.my_lovely_team.my_lovely_project.my_lovely_component import (
    MyLovelyCompanyTeamProjectComponent as component,  # DRY
)


class C:
    @pytest.mark.parametrize(
        ("post_data", "message"),
        [
            # metadata_version errors.
            (
                {},
                "None is an invalid value for Metadata-Version. Error: This field is"
                " required. see"
                " https://packaging.python.org/specifications/core-metadata",
            ),
            (
                {"metadata_version": "-1"},
                "'-1' is an invalid value for Metadata-Version. Error: Unknown Metadata"
                " Version see"
                " https://packaging.python.org/specifications/core-metadata",
            ),
            # name errors.
            (
                {"metadata_version": "1.2"},
                "'' is an invalid value for Name. Error: This field is required. see"
                " https://packaging.python.org/specifications/core-metadata",
            ),
            (
                {"metadata_version": "1.2", "name": "foo-"},
                "'foo-' is an invalid value for Name. Error: Must start and end with a"
                " letter or numeral and contain only ascii numeric and '.', '_' and"
                " '-'. see https://packaging.python.org/specifications/core-metadata",
            ),
            # version errors.
            (
                {"metadata_version": "1.2", "name": "example"},
                "'' is an invalid value for Version. Error: This field is required. see"
                " https://packaging.python.org/specifications/core-metadata",
            ),
            (
                {"metadata_version": "1.2", "name": "example", "version": "dog"},
                "'dog' is an invalid value for Version. Error: Must start and end with"
                " a letter or numeral and contain only ascii numeric and '.', '_' and"
                " '-'. see https://packaging.python.org/specifications/core-metadata",
            ),
        ],
    )
    def test_fails_invalid_post_data(
        self, pyramid_config, db_request, post_data, message
    ):
        pyramid_config.testing_securitypolicy(userid=1)
        db_request.POST = MultiDict(post_data)


def foo(list_a, list_b):
    results = (
        User.query.filter(User.foo == "bar")
        .filter(  # Because foo.
            db.or_(User.field_a.astext.in_(list_a), User.field_b.astext.in_(list_b))
        )
        .filter(User.xyz.is_(None))
        # Another comment about the filtering on is_quux goes here.
        .filter(db.not_(User.is_pending.astext.cast(db.Boolean).is_(True)))
        .order_by(User.created_at.desc())
        .with_for_update(key_share=True)
        .all()
    )
    return results


def foo2(list_a, list_b):
    # Standalone comment reasonably placed.
    return (
        User.query.filter(User.foo == "bar")
        .filter(
            db.or_(User.field_a.astext.in_(list_a), User.field_b.astext.in_(list_b))
        )
        .filter(User.xyz.is_(None))
    )


def foo3(list_a, list_b):
    return (
        # Standalone comment but weirdly placed.
        User.query.filter(User.foo == "bar")
        .filter(
            db.or_(User.field_a.astext.in_(list_a), User.field_b.astext.in_(list_b))
        )
        .filter(User.xyz.is_(None))
    )
